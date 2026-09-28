//! Minimal ECMA-style regex subset for JSON Schema `pattern` in tests:
//! literals, `.`, `^`, `$`, classes (`[a-z0-9_-]`, `[^...]`), `\d \w \s` and
//! escaped literals, groups `( )`/`(?: )` with `|`, and `* + ? {n} {n,} {n,m}`.
//! Unsupported syntax panics so a schema cannot silently rely on it.

use std::collections::BTreeSet;

#[derive(Clone, Debug)]
enum ClassItem {
    Char(char),
    Range(char, char),
    Digit,
    Word,
    Space,
}

#[derive(Clone, Debug)]
enum Node {
    Char(char),
    Any,
    Class {
        negated: bool,
        items: Vec<ClassItem>,
    },
    Start,
    End,
    Group(Vec<Vec<Node>>),
    Repeat {
        node: Box<Node>,
        min: usize,
        max: Option<usize>,
    },
}

pub struct Regex(Vec<Vec<Node>>);

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    source: &'a str,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        self.pos += 1;
        c
    }

    fn alternatives(&mut self) -> Vec<Vec<Node>> {
        let mut alts = vec![Vec::new()];
        while let Some(c) = self.peek() {
            match c {
                ')' => break,
                '|' => {
                    self.pos += 1;
                    alts.push(Vec::new());
                }
                _ => {
                    let atom = self.atom();
                    let atom = self.quantifier(atom);
                    alts.last_mut().unwrap().push(atom);
                }
            }
        }
        alts
    }

    fn escape_item(&mut self) -> ClassItem {
        match self.next() {
            Some('d') => ClassItem::Digit,
            Some('w') => ClassItem::Word,
            Some('s') => ClassItem::Space,
            Some(c) if "\\/.-^$[](){}|*+?".contains(c) => ClassItem::Char(c),
            other => panic!("unsupported escape {other:?} in {}", self.source),
        }
    }

    fn atom(&mut self) -> Node {
        match self.next().unwrap() {
            '.' => Node::Any,
            '^' => Node::Start,
            '$' => Node::End,
            '\\' => match self.escape_item() {
                ClassItem::Char(c) => Node::Char(c),
                item => Node::Class {
                    negated: false,
                    items: vec![item],
                },
            },
            '(' => {
                if self.peek() == Some('?') {
                    self.pos += 1;
                    assert_eq!(
                        self.next(),
                        Some(':'),
                        "unsupported group in {}",
                        self.source
                    );
                }
                let alts = self.alternatives();
                assert_eq!(
                    self.next(),
                    Some(')'),
                    "unbalanced group in {}",
                    self.source
                );
                Node::Group(alts)
            }
            '[' => {
                let negated = self.peek() == Some('^');
                if negated {
                    self.pos += 1;
                }
                let mut items = Vec::new();
                loop {
                    let c = self
                        .next()
                        .unwrap_or_else(|| panic!("open class in {}", self.source));
                    if c == ']' {
                        break;
                    }
                    let item = if c == '\\' {
                        self.escape_item()
                    } else {
                        ClassItem::Char(c)
                    };
                    if let ClassItem::Char(low) = item
                        && self.peek() == Some('-')
                        && self.chars.get(self.pos + 1).is_some_and(|n| *n != ']')
                    {
                        self.pos += 1;
                        let high = self.next().unwrap();
                        items.push(ClassItem::Range(low, high));
                    } else {
                        items.push(item);
                    }
                }
                Node::Class { negated, items }
            }
            c if "*+?{}|)".contains(c) => panic!("unexpected {c} in {}", self.source),
            c => Node::Char(c),
        }
    }

    fn number(&mut self) -> Option<usize> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        (self.pos > start).then(|| {
            self.chars[start..self.pos]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap()
        })
    }

    fn quantifier(&mut self, atom: Node) -> Node {
        let (min, max) = match self.peek() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            Some('{') => {
                self.pos += 1;
                let min = self.number().expect("quantifier minimum");
                let max = if self.peek() == Some(',') {
                    self.pos += 1;
                    self.number()
                } else {
                    Some(min)
                };
                assert_eq!(self.peek(), Some('}'), "bad quantifier in {}", self.source);
                (min, max)
            }
            _ => return atom,
        };
        self.pos += 1;
        Node::Repeat {
            node: Box::new(atom),
            min,
            max,
        }
    }
}

fn class_matches(items: &[ClassItem], c: char) -> bool {
    items.iter().any(|item| match item {
        ClassItem::Char(x) => *x == c,
        ClassItem::Range(a, b) => (*a..=*b).contains(&c),
        ClassItem::Digit => c.is_ascii_digit(),
        ClassItem::Word => c.is_ascii_alphanumeric() || c == '_',
        ClassItem::Space => c.is_whitespace(),
    })
}

fn match_node(node: &Node, s: &[char], pos: usize) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    match node {
        Node::Char(c) => {
            if s.get(pos) == Some(c) {
                out.insert(pos + 1);
            }
        }
        Node::Any => {
            if s.get(pos).is_some_and(|c| *c != '\n' && *c != '\r') {
                out.insert(pos + 1);
            }
        }
        Node::Class { negated, items } => {
            if let Some(c) = s.get(pos)
                && class_matches(items, *c) != *negated
            {
                out.insert(pos + 1);
            }
        }
        Node::Start => {
            if pos == 0 {
                out.insert(pos);
            }
        }
        Node::End => {
            if pos == s.len() {
                out.insert(pos);
            }
        }
        Node::Group(alts) => {
            for alt in alts {
                out.extend(match_seq(alt, s, pos));
            }
        }
        Node::Repeat { node, min, max } => {
            let mut current: BTreeSet<usize> = [pos].into();
            let limit = max.unwrap_or(s.len() + 1);
            for count in 0..=limit {
                if count >= *min {
                    out.extend(current.iter().copied());
                }
                if count == limit {
                    break;
                }
                let next: BTreeSet<usize> = current
                    .iter()
                    .flat_map(|p| match_node(node, s, *p))
                    .collect();
                if next.is_empty() || (count >= *min && next.is_subset(&out)) {
                    break;
                }
                current = next;
            }
        }
    }
    out
}

fn match_seq(nodes: &[Node], s: &[char], pos: usize) -> BTreeSet<usize> {
    let mut current: BTreeSet<usize> = [pos].into();
    for node in nodes {
        current = current
            .iter()
            .flat_map(|p| match_node(node, s, *p))
            .collect();
        if current.is_empty() {
            break;
        }
    }
    current
}

impl Regex {
    pub fn new(pattern: &str) -> Self {
        let mut parser = Parser {
            chars: pattern.chars().collect(),
            pos: 0,
            source: pattern,
        };
        let alts = parser.alternatives();
        assert!(parser.peek().is_none(), "unbalanced ) in {pattern}");
        Self(alts)
    }

    /// ECMA `pattern` semantics: unanchored search unless `^`/`$` are used.
    pub fn is_match(&self, text: &str) -> bool {
        let s: Vec<char> = text.chars().collect();
        (0..=s.len()).any(|start| {
            self.0
                .iter()
                .any(|alt| !match_seq(alt, &s, start).is_empty())
        })
    }
}
