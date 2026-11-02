use super::*;
fn assert_quote(source: &str, hint: &str, quote: &str) {
    let result = locate(source, hint).unwrap();
    assert_eq!(result.algorithm, ALGORITHM);
    assert_eq!(result.quote, quote);
    assert_eq!(
        source
            .chars()
            .skip(result.start as usize)
            .take((result.end - result.start) as usize)
            .collect::<String>(),
        quote
    );
}
#[test]
fn original_scalars_survive_width_whitespace_quotes_and_emoji() {
    let source = "🙂前言：我说「ＡＢＣ\u{3000}\t散步」。";
    assert_quote(source, "我说\"ABC 散步\"", "我说「ＡＢＣ\u{3000}\t散步」");
    assert_eq!(locate(source, "我说").unwrap().start, 4);
    assert_quote("a ‘today’ b", "'today'", "‘today’");
    assert_quote("a 『明日』 b", "\"明日\"", "『明日』");
    assert_quote(
        "a hello\r\n \tworld b",
        "  hello world\t",
        "hello\r\n \tworld",
    );
}
#[test]
fn compatibility_canonical_japanese_and_hangul_map_back_to_whole_original() {
    for (source, hint) in [
        ("前ｶﾞ後", "ガ"),
        ("前ガ後", "ｶﾞ"),
        ("前café後", "cafe\u{301}"),
        ("前cafe\u{301}後", "café"),
        ("前각後", "\u{1100}\u{1161}\u{11a8}"),
        ("前\u{1100}\u{1161}\u{11a8}後", "각"),
        ("前ﬃ後", "ffi"),
        ("前①後", "1"),
    ] {
        let quote: String = source
            .chars()
            .skip(1)
            .take(source.chars().count() - 2)
            .collect();
        assert_quote(source, hint, &quote);
    }
}
#[test]
fn canonical_reordering_cannot_manufacture_a_discontiguous_quote() {
    assert_quote("a\u{315}\u{300}", "à\u{315}", "a\u{315}\u{300}");
    assert_quote("\u{315}\u{300}x", "\u{300}\u{315}", "\u{315}\u{300}");
    assert_eq!(locate("a\u{315}\u{300}", "à"), Err(LocateFault::NotFound));
    for hint in ["f", "ff", "fi"] {
        assert_eq!(locate("ﬃ", hint), Err(LocateFault::NotFound));
    }
}
#[test]
fn repeated_overlapping_and_normalized_equal_spans_are_ambiguous() {
    for (source, hint) in [("散步，散步", "散步"), ("aaa", "aa"), ("ＡＢ AB", "AB")] {
        assert_eq!(locate(source, hint), Err(LocateFault::Ambiguous));
    }
    assert_eq!(locate("Ｆﬁ", "f"), Err(LocateFault::NotFound));
}
#[test]
fn never_folds_case_rewords_punctuation_or_invents_a_missing_quote() {
    for (source, hint) in [
        ("Today", "today"),
        ("一起散步", "一起跑步"),
        ("a,b", "ab"),
        ("hello", "\"hello\""),
    ] {
        assert_eq!(locate(source, hint), Err(LocateFault::NotFound));
    }
    assert_eq!(locate("", "hello"), Err(LocateFault::NotFound));
}
#[test]
fn input_and_original_evidence_budgets_are_enforced() {
    for hint in ["", " \r\n\t\u{3000}"] {
        assert_eq!(locate("text", hint), Err(LocateFault::InvalidInput));
    }
    assert_eq!(
        locate(&"a".repeat(MAX_SOURCE_BYTES + 1), "a"),
        Err(LocateFault::InvalidInput)
    );
    assert_eq!(
        locate("text", &"a".repeat(501)),
        Err(LocateFault::InvalidInput)
    );
    // Hint is short, but the exact original span exceeds strict evidence's 500 scalars.
    assert_eq!(
        locate(&format!("a{}b", " ".repeat(501)), "a b"),
        Err(LocateFault::NotFound)
    );
    let source = format!("{}x", "a".repeat(MAX_SOURCE_BYTES - 1));
    assert_quote(&source, "x", "x");
}

#[test]
fn located_original_passes_strict_evidence_but_normalized_text_and_spoofed_speaker_do_not() {
    use enouia_memory_contract::{common::*, hash::sha256, next::*};
    let mut source: CapturedSource = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-records/source.json"
    ))
    .unwrap();
    source.text = "🙂我说「ＡＢＣ\u{3000}散步」".into();
    source.record.content_hash = sha256(source.text.as_bytes());
    let found = locate(&source.text, "我说\"ABC 散步\"").unwrap();
    let input: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-records/input.json"
    ))
    .unwrap();
    let mut evidence: QuotedEvidence =
        serde_json::from_value(input["record_fields"]["evidence"][0].clone()).unwrap();
    evidence.reference.object_hash = source.record.content_hash.clone();
    evidence.quote = found.quote;
    evidence.start = found.start;
    evidence.end = found.end;
    assert!(evidence.resolves(&[source.clone()]));
    let mut normalized = evidence.clone();
    normalized.quote = "我说\"ABC 散步\"".into();
    assert!(!normalized.resolves(&[source.clone()]));
    source.record.speaker_role = SpeakerRole::Assistant;
    source.record.evidence_class = EvidenceClass::ModelClaim;
    assert!(!evidence.resolves(&[source.clone()]));
    evidence.speaker = SpeakerRole::Assistant;
    evidence.reference.evidence_class = EvidenceClass::ModelClaim;
    assert!(evidence.resolves(&[source.clone()]));
    source.record.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    assert!(!evidence.resolves(&[source]));
}

#[test]
fn mapped_order_agrees_with_icu_for_mixed_starters_marks_and_compatibility_forms() {
    let alphabet = [
        'A', 'é', '\u{300}', '\u{315}', '\u{327}', '각', '\u{1100}', '\u{1161}', 'ｶ', 'ﾞ', 'ﬃ', '①',
        '\u{fdfa}', '🙂', '\t', '\u{3000}',
    ];
    for a in alphabet {
        for b in alphabet {
            for c in alphabet {
                assert!(mapped(&format!("{a}{b}{c}")).is_ok());
            }
        }
    }
}
