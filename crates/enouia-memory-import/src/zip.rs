//! A small, bounded ZIP reader for received exports (IMPORT_REVIEW §2.1, I05).
//!
//! Only what exports need: the central directory, stored and deflate
//! members, CRC-32 verification. Everything else is refused or quarantined
//! instead of guessed: ZIP64, multi-disk, encryption, symlinks, unsafe or
//! duplicate names, overlapping members, and members whose declared or
//! actual size or compression ratio exceed the limits (zip bombs). Nothing is
//! ever written to disk under a member name: members are returned as bytes.

use enouia_memory_contract::import::is_safe_member_name;

#[derive(Clone, Debug)]
pub struct ZipLimits {
    pub max_members: usize,
    pub max_member_bytes: u64,
    pub max_total_bytes: u64,
    /// Uncompressed / compressed, checked for members above 1 MiB.
    pub max_ratio: u64,
}

impl Default for ZipLimits {
    fn default() -> Self {
        Self {
            max_members: 20_000,
            max_member_bytes: 512 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_ratio: 200,
        }
    }
}

/// The whole archive cannot be read. Its bytes stay archived as received.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveError {
    NotZip,
    Truncated,
    Zip64Unsupported,
    MultiDiskUnsupported,
    TooManyMembers,
    TotalTooLarge,
    OverlappingMembers,
}

impl ArchiveError {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotZip => "not_zip",
            Self::Truncated => "archive_truncated",
            Self::Zip64Unsupported => "zip64_unsupported",
            Self::MultiDiskUnsupported => "multi_disk_unsupported",
            Self::TooManyMembers => "too_many_members",
            Self::TotalTooLarge => "archive_too_large",
            Self::OverlappingMembers => "overlapping_members",
        }
    }
}

/// Why one member was not extracted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemberProblem {
    UnsafeName,
    DuplicateName,
    Encrypted,
    Symlink,
    UnsupportedMethod,
    TooLarge,
    RatioExceeded,
    Corrupt,
    CrcMismatch,
}

impl MemberProblem {
    pub fn code(self) -> &'static str {
        match self {
            Self::UnsafeName => "unsafe_member_name",
            Self::DuplicateName => "duplicate_member_name",
            Self::Encrypted => "encrypted_member",
            Self::Symlink => "symlink_member",
            Self::UnsupportedMethod => "unsupported_compression",
            Self::TooLarge => "member_too_large",
            Self::RatioExceeded => "compression_ratio_exceeded",
            Self::Corrupt => "member_corrupt",
            Self::CrcMismatch => "member_crc_mismatch",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    /// The member name as stored (lossy UTF-8), used only for display
    /// after sanitization checks; never used as a file-system path.
    pub name: String,
    pub is_dir: bool,
    method: u16,
    flags: u16,
    crc32: u32,
    compressed: u64,
    uncompressed: u64,
    local_offset: u64,
    symlink: bool,
    pub problem: Option<MemberProblem>,
}

impl Entry {
    pub fn uncompressed_size(&self) -> u64 {
        self.uncompressed
    }
}

pub struct Archive<'a> {
    bytes: &'a [u8],
    pub entries: Vec<Entry>,
    limits: ZipLimits,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;

pub fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && u32_at(bytes, 0) == Some(LOCAL)
        || bytes.len() >= 22 && u32_at(bytes, 0) == Some(EOCD)
}

impl<'a> Archive<'a> {
    pub fn open(bytes: &'a [u8], limits: ZipLimits) -> Result<Self, ArchiveError> {
        if bytes.len() < 22 {
            return Err(ArchiveError::NotZip);
        }
        let search_from = bytes.len().saturating_sub(22 + 65_535);
        let eocd = (search_from..=bytes.len() - 22)
            .rev()
            .find(|&at| u32_at(bytes, at) == Some(EOCD))
            .ok_or(ArchiveError::NotZip)?;
        if eocd >= 20 && u32_at(bytes, eocd - 20) == Some(ZIP64_LOCATOR) {
            return Err(ArchiveError::Zip64Unsupported);
        }
        let disk = u16_at(bytes, eocd + 4).ok_or(ArchiveError::Truncated)?;
        let cd_disk = u16_at(bytes, eocd + 6).ok_or(ArchiveError::Truncated)?;
        let count_disk = u16_at(bytes, eocd + 8).ok_or(ArchiveError::Truncated)?;
        let count = u16_at(bytes, eocd + 10).ok_or(ArchiveError::Truncated)?;
        let cd_size = u32_at(bytes, eocd + 12).ok_or(ArchiveError::Truncated)?;
        let cd_offset = u32_at(bytes, eocd + 16).ok_or(ArchiveError::Truncated)?;
        if disk != 0 || cd_disk != 0 || count_disk != count {
            return Err(ArchiveError::MultiDiskUnsupported);
        }
        if count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF {
            return Err(ArchiveError::Zip64Unsupported);
        }
        if count as usize > limits.max_members {
            return Err(ArchiveError::TooManyMembers);
        }
        let mut at = cd_offset as usize;
        let end = at
            .checked_add(cd_size as usize)
            .filter(|&e| e <= eocd)
            .ok_or(ArchiveError::Truncated)?;
        let mut entries = Vec::with_capacity(count as usize);
        let mut total: u64 = 0;
        for _ in 0..count {
            if u32_at(bytes, at) != Some(CENTRAL) {
                return Err(ArchiveError::Truncated);
            }
            let made_by = u16_at(bytes, at + 4).ok_or(ArchiveError::Truncated)?;
            let flags = u16_at(bytes, at + 8).ok_or(ArchiveError::Truncated)?;
            let method = u16_at(bytes, at + 10).ok_or(ArchiveError::Truncated)?;
            let crc32 = u32_at(bytes, at + 16).ok_or(ArchiveError::Truncated)?;
            let compressed = u32_at(bytes, at + 20).ok_or(ArchiveError::Truncated)?;
            let uncompressed = u32_at(bytes, at + 24).ok_or(ArchiveError::Truncated)?;
            let name_len = u16_at(bytes, at + 28).ok_or(ArchiveError::Truncated)? as usize;
            let extra_len = u16_at(bytes, at + 30).ok_or(ArchiveError::Truncated)? as usize;
            let comment_len = u16_at(bytes, at + 32).ok_or(ArchiveError::Truncated)? as usize;
            let external = u32_at(bytes, at + 38).ok_or(ArchiveError::Truncated)?;
            let local = u32_at(bytes, at + 42).ok_or(ArchiveError::Truncated)?;
            if compressed == 0xFFFF_FFFF || uncompressed == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
                return Err(ArchiveError::Zip64Unsupported);
            }
            let name_bytes = bytes
                .get(at + 46..at + 46 + name_len)
                .ok_or(ArchiveError::Truncated)?;
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            let is_dir = name.ends_with('/');
            // Unix mode in the high 16 bits when made on Unix (host 3).
            let symlink = made_by >> 8 == 3 && (external >> 16) & 0o170_000 == 0o120_000;
            total = total.saturating_add(uncompressed as u64);
            entries.push(Entry {
                name,
                is_dir,
                method,
                flags,
                crc32,
                compressed: compressed as u64,
                uncompressed: uncompressed as u64,
                local_offset: local as u64,
                symlink,
                problem: None,
            });
            at = at + 46 + name_len + extra_len + comment_len;
            if at > end {
                return Err(ArchiveError::Truncated);
            }
        }
        if total > limits.max_total_bytes {
            return Err(ArchiveError::TotalTooLarge);
        }
        // Overlapping member data is a known zip-bomb construction.
        let mut spans: Vec<(u64, u64)> = entries
            .iter()
            .map(|e| (e.local_offset, e.local_offset + 30 + e.compressed))
            .collect();
        spans.sort();
        if spans.windows(2).any(|w| w[1].0 < w[0].1) {
            return Err(ArchiveError::OverlappingMembers);
        }
        let mut names = std::collections::BTreeSet::new();
        for entry in &mut entries {
            let trimmed = entry.name.trim_end_matches('/');
            entry.problem = if !is_safe_member_name(trimmed) {
                Some(MemberProblem::UnsafeName)
            } else if !names.insert(trimmed.to_lowercase()) {
                Some(MemberProblem::DuplicateName)
            } else if entry.flags & 1 != 0 {
                Some(MemberProblem::Encrypted)
            } else if entry.symlink {
                Some(MemberProblem::Symlink)
            } else if entry.method != 0 && entry.method != 8 {
                Some(MemberProblem::UnsupportedMethod)
            } else if entry.uncompressed > limits.max_member_bytes {
                Some(MemberProblem::TooLarge)
            } else if entry.uncompressed > 1024 * 1024
                && entry.uncompressed / entry.compressed.max(1) > limits.max_ratio
            {
                Some(MemberProblem::RatioExceeded)
            } else {
                None
            };
        }
        Ok(Self {
            bytes,
            entries,
            limits,
        })
    }

    /// Decompress one member, bounded by its declared size, and verify its
    /// length and CRC-32. A problem found here quarantines the member only.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, MemberProblem> {
        if let Some(problem) = entry.problem {
            return Err(problem);
        }
        let at = entry.local_offset as usize;
        if u32_at(self.bytes, at) != Some(LOCAL) {
            return Err(MemberProblem::Corrupt);
        }
        let name_len = u16_at(self.bytes, at + 26).ok_or(MemberProblem::Corrupt)? as usize;
        let extra_len = u16_at(self.bytes, at + 28).ok_or(MemberProblem::Corrupt)? as usize;
        let start = at + 30 + name_len + extra_len;
        let data = self
            .bytes
            .get(start..start + entry.compressed as usize)
            .ok_or(MemberProblem::Corrupt)?;
        let limit = entry.uncompressed.min(self.limits.max_member_bytes) as usize;
        let out = match entry.method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit)
                .map_err(|_| MemberProblem::Corrupt)?,
            _ => return Err(MemberProblem::UnsupportedMethod),
        };
        if out.len() as u64 != entry.uncompressed {
            return Err(MemberProblem::Corrupt);
        }
        if crc32fast::hash(&out) != entry.crc32 {
            return Err(MemberProblem::CrcMismatch);
        }
        Ok(out)
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.name == name && e.problem.is_none())
    }
}
