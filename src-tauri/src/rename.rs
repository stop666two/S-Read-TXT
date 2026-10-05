//! 批量重命名引擎：规则计算（计划）与两阶段改名（应用/撤销）。
//!
//! 计划阶段纯计算不触盘（「目标已存在」判定使用目录扫描结果）；
//! 应用阶段采用「先全部改临时名，再改目标名」两阶段，任一步失败按逆序回滚。

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// 序号插入位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberAt {
    /// 插在茎部之前。
    Prefix,
    /// 插在茎部之后（扩展名前）。
    Suffix,
}

/// 序号规则。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Numbering {
    /// 起始序号。
    pub start: u32,
    /// 步进。
    pub step: u32,
    /// 补零位数（2..=6；超出位数时自然加宽）。
    pub digits: u8,
    /// 插入位置。
    pub at: NumberAt,
}

/// 批量重命名规则（仅作用于茎部，不含扩展名）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRules {
    /// 茎部查找文本（大小写敏感）；空/缺省表示不替换。
    pub find: Option<String>,
    /// 替换文本（可为空串表示删除）。
    pub replace: String,
    /// 茎部前缀。
    pub prefix: Option<String>,
    /// 茎部后缀。
    pub suffix: Option<String>,
    /// 序号插入规则。
    pub numbering: Option<Numbering>,
}

/// 单条重命名状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenameStatus {
    /// 可执行。
    Ok,
    /// 新旧完全相同（无需改名）。
    Unchanged,
    /// 多处目标同名。
    Conflict,
    /// 目标已存在于目录（且不在被改集合）。
    Exists,
    /// 目标名非法（字符/保留名/长度等）。
    Invalid,
}

/// 单条重命名计划项。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEntry {
    /// 旧文件名。
    pub old: String,
    /// 新文件名。
    pub new: String,
    /// 状态。
    pub status: RenameStatus,
}

/// 重命名错误。
#[derive(Debug)]
pub enum RenameError {
    /// 规则非法（序号位数越界等）。
    InvalidConfig(String),
    /// 扫描结果超出上限。
    TooMany(usize),
    /// 计划中存在不可执行项（冲突/已存在/非法）。
    Blocked,
    /// IO 失败（附阶段说明）。
    Io(String, io::Error),
    /// 回滚也失败（原始状态可能不完整）。
    Rollback(String),
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenameError::InvalidConfig(msg) => write!(f, "规则非法：{msg}"),
            RenameError::TooMany(count) => write!(f, "文件数超出上限（{count}）"),
            RenameError::Blocked => write!(f, "存在冲突、重名或非法目标名，已拒绝执行"),
            RenameError::Io(stage, err) => write!(f, "{stage}失败：{err}"),
            RenameError::Rollback(stage) => write!(f, "{stage}；且回滚未完全成功"),
        }
    }
}

impl std::error::Error for RenameError {}

/// 扩展名白名单（小写比较）。
pub fn default_extensions() -> Vec<String> {
    vec!["txt".to_string(), "log".to_string()]
}

/// 扫描目录（仅文件、扩展名白名单、自然序）；超出上限返回 `TooMany`。
pub fn scan(dir: &Path, extensions: &[String], cap: usize) -> Result<Vec<String>, RenameError> {
    let reader = fs::read_dir(dir).map_err(|err| RenameError::Io("读取目录".into(), err))?;
    let mut names = Vec::new();
    for item in reader {
        let item = item.map_err(|err| RenameError::Io("读取目录项".into(), err))?;
        let path = item.path();
        let is_file = item
            .file_type()
            .map_err(|err| RenameError::Io("读取文件类型".into(), err))?
            .is_file();
        if !is_file {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let ext = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        let matched = ext
            .as_deref()
            .map(|ext| {
                extensions
                    .iter()
                    .any(|allow| allow.eq_ignore_ascii_case(ext))
            })
            .unwrap_or(false);
        if matched {
            names.push(name.to_string());
        }
    }
    names.sort_by(|a, b| natural_cmp(a, b));
    if names.len() > cap {
        return Err(RenameError::TooMany(names.len()));
    }
    Ok(names)
}

/// 计算重命名计划（纯函数；不改动磁盘）。
pub fn plan(dir: &Path, files: &[String], rules: &RenameRules) -> Vec<RenameEntry> {
    if let Some(Numbering { digits, .. }) = rules.numbering {
        if !(2..=6).contains(&digits) {
            return Vec::new();
        }
    }
    let mut ordered: Vec<String> = files.to_vec();
    ordered.sort_by(|a, b| natural_cmp(a, b));

    let mut planned: Vec<RenameEntry> = Vec::with_capacity(ordered.len());
    for (index, old) in ordered.iter().enumerate() {
        let new = compute_new_name(old, rules, index);
        planned.push(RenameEntry {
            old: old.clone(),
            new,
            status: RenameStatus::Ok,
        });
    }

    let existing: Vec<String> = fs::read_dir(dir)
        .map(|reader| {
            reader
                .filter_map(Result::ok)
                .filter_map(|item| item.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut target_counts: HashMap<String, usize> = HashMap::new();
    for entry in &planned {
        if entry.status == RenameStatus::Invalid || entry.old == entry.new {
            continue;
        }
        *target_counts.entry(entry.new.to_lowercase()).or_insert(0) += 1;
    }
    let vacating: Vec<String> = planned
        .iter()
        .filter(|entry| entry.status != RenameStatus::Invalid && entry.old != entry.new)
        .map(|entry| entry.old.to_lowercase())
        .collect();

    for entry in &mut planned {
        if entry.old == entry.new {
            entry.status = RenameStatus::Unchanged;
            continue;
        }
        if !is_valid_name(&entry.new) {
            entry.status = RenameStatus::Invalid;
            continue;
        }
        if target_counts
            .get(&entry.new.to_lowercase())
            .copied()
            .unwrap_or(0)
            > 1
        {
            entry.status = RenameStatus::Conflict;
            continue;
        }
        let new_lower = entry.new.to_lowercase();
        let exists_on_disk = existing.iter().any(|name| name.to_lowercase() == new_lower);
        let same_as_old = entry.old.to_lowercase() == new_lower;
        let among_vacating = vacating.iter().any(|name| name == &new_lower);
        if exists_on_disk && !same_as_old && !among_vacating {
            entry.status = RenameStatus::Exists;
        }
    }
    planned
}

/// 两阶段应用：`old → 临时名 → new`；失败逆序回滚到原始状态。
pub fn apply(dir: &Path, pairs: &[(String, String)]) -> Result<Vec<(String, String)>, RenameError> {
    apply_with(dir, pairs, &mut |from, to| fs::rename(from, to))
}

/// 注入式应用（测试注入失败点）；`op` 为改名原语。
pub fn apply_with(
    dir: &Path,
    pairs: &[(String, String)],
    op: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<Vec<(String, String)>, RenameError> {
    let pid = std::process::id();
    let tmp_name = |index: usize, old: &str| format!(".{old}.srt-ren-{pid}-{index}");

    // 阶段一：old → 临时名；失败则逆序还原已暂存项
    for (index, (old, _)) in pairs.iter().enumerate() {
        let from = dir.join(old);
        let to = dir.join(tmp_name(index, old));
        if let Err(err) = op(&from, &to) {
            for (prev_index, (prev_old, _)) in pairs[..index].iter().enumerate().rev() {
                let _ = op(
                    &dir.join(tmp_name(prev_index, prev_old)),
                    &dir.join(prev_old),
                );
            }
            return Err(RenameError::Io(format!("暂存 {old}"), err));
        }
    }

    // 阶段二：临时名 → 目标名；失败则「已提交逆序还原 + 未提交暂存还原」
    for (index, (old, new)) in pairs.iter().enumerate() {
        if let Err(err) = op(&dir.join(tmp_name(index, old)), &dir.join(new)) {
            let mut rollback_failed = false;
            for (_, (prev_old, prev_new)) in pairs[..index].iter().enumerate().rev() {
                if op(&dir.join(prev_new), &dir.join(prev_old)).is_err() {
                    rollback_failed = true;
                }
            }
            for (offset, (pending_old, _)) in pairs[index..].iter().enumerate() {
                let pending_index = index + offset;
                if op(
                    &dir.join(tmp_name(pending_index, pending_old)),
                    &dir.join(pending_old),
                )
                .is_err()
                {
                    rollback_failed = true;
                }
            }
            return if rollback_failed {
                Err(RenameError::Rollback(format!("改名为 {new}")))
            } else {
                Err(RenameError::Io(format!("改名为 {new}"), err))
            };
        }
    }
    Ok(pairs.to_vec())
}

/// 撤销 = 反向应用（`new → old`）。
pub fn undo(dir: &Path, pairs: &[(String, String)]) -> Result<(), RenameError> {
    let reversed: Vec<(String, String)> = pairs
        .iter()
        .map(|(old, new)| (new.clone(), old.clone()))
        .collect();
    apply(dir, &reversed).map(|_| ())
}

/// 计算新文件名：替换（茎部）→ 前缀/后缀 → 序号（排序后按序号递增）。
fn compute_new_name(old: &str, rules: &RenameRules, index: usize) -> String {
    let (stem, ext) = match old.rsplit_once('.') {
        Some((stem, ext)) if !ext.is_empty() => (stem.to_string(), Some(ext.to_string())),
        _ => (old.to_string(), None),
    };
    let mut next = stem;
    if let Some(find) = rules.find.as_deref() {
        if !find.is_empty() {
            next = next.replace(find, &rules.replace);
        }
    }
    if let Some(prefix) = rules.prefix.as_deref() {
        next = format!("{prefix}{next}");
    }
    if let Some(suffix) = rules.suffix.as_deref() {
        next = format!("{next}{suffix}");
    }
    if let Some(numbering) = rules.numbering.as_ref() {
        let value = numbering
            .start
            .saturating_add(numbering.step.saturating_mul(index as u32));
        let label = format!("{:0width$}", value, width = numbering.digits as usize);
        next = match numbering.at {
            NumberAt::Prefix => format!("{label}{next}"),
            NumberAt::Suffix => format!("{next}{label}"),
        };
    }
    match ext {
        Some(ext) => format!("{next}.{ext}"),
        None => next,
    }
}

/// 目标名合法性：非法字符、结尾点/空格、保留设备名、长度上限。
fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 240 {
        return false;
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    if name.chars().any(|ch| {
        ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return false;
    }
    let base = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    !RESERVED.contains(&base.as_str())
}

/// 自然序比较（数字段按数值比较，其余按小写字典序）。
fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let a = left.to_lowercase();
    let b = right.to_lowercase();
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na = take_number(&mut ai);
                    let nb = take_number(&mut bi);
                    match na.cmp(&nb) {
                        std::cmp::Ordering::Equal => continue,
                        other => return other,
                    }
                }
                if ca != cb {
                    return ca.cmp(&cb);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

fn take_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u128 {
    let mut value: u128 = 0;
    while let Some(ch) = chars.peek().copied() {
        if let Some(digit) = ch.to_digit(10) {
            value = value.saturating_mul(10).saturating_add(u128::from(digit));
            chars.next();
        } else {
            break;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn touch(dir: &Path, name: &str) {
        fs::write(dir.join(name), "x").unwrap();
    }

    #[test]
    fn plan_applies_replace_prefix_suffix_and_numbering() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        for name in ["a.txt", "b.txt"] {
            touch(dir, name);
        }
        let rules = RenameRules {
            find: Some("a".into()),
            replace: "alpha".into(),
            prefix: Some("p-".into()),
            suffix: Some("-s".into()),
            numbering: Some(Numbering {
                start: 5,
                step: 5,
                digits: 3,
                at: NumberAt::Prefix,
            }),
        };
        let plan = plan(dir, &["b.txt".into(), "a.txt".into()], &rules);
        assert_eq!(plan[0].old, "a.txt");
        assert_eq!(plan[0].new, "005p-alpha-s.txt");
        assert_eq!(plan[1].old, "b.txt");
        assert_eq!(plan[1].new, "010p-b-s.txt");
        assert!(plan.iter().all(|entry| entry.status == RenameStatus::Ok));
    }

    #[test]
    fn plan_marks_conflicts_when_targets_collide() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        touch(dir, "a.txt");
        // 重复源名（不应来自 UI，但纯函数层需拒绝重名目标）
        let rules = RenameRules {
            find: Some("a".into()),
            replace: "z".into(),
            prefix: None,
            suffix: None,
            numbering: None,
        };
        let plan = plan(dir, &["a.txt".into(), "a.txt".into()], &rules);
        assert!(plan
            .iter()
            .all(|entry| entry.status == RenameStatus::Conflict));
    }

    #[test]
    fn plan_marks_exists_and_invalid_and_unchanged() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        for name in ["x.txt", "y.txt", "keep.txt", "bad.lnk"] {
            touch(dir, name);
        }
        let rules = RenameRules {
            find: Some("y".into()),
            replace: "keep".into(), // y.txt → keep.txt（磁盘已存在且 keep.txt 未被改名）
            prefix: None,
            suffix: None,
            numbering: None,
        };
        let planned = plan(
            dir,
            &["x.txt".into(), "y.txt".into(), "keep.txt".into()],
            &rules,
        );
        let by_old: HashMap<String, RenameEntry> = planned
            .into_iter()
            .map(|entry| (entry.old.clone(), entry))
            .collect();
        assert_eq!(by_old["y.txt"].status, RenameStatus::Exists);
        assert_eq!(by_old["x.txt"].status, RenameStatus::Unchanged);
        assert_eq!(by_old["keep.txt"].status, RenameStatus::Unchanged);

        let invalid_rules = RenameRules {
            find: Some("bad".into()),
            replace: "CON".into(), // bad.lnk → CON.lnk（保留设备名）
            prefix: None,
            suffix: None,
            numbering: None,
        };
        let planned = plan(dir, &["bad.lnk".into()], &invalid_rules);
        assert_eq!(planned[0].new, "CON.lnk");
        assert_eq!(planned[0].status, RenameStatus::Invalid);
    }

    #[test]
    fn plan_treats_case_only_change_as_ok() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        touch(dir, "name.txt");
        let rules = RenameRules {
            find: Some("name".into()),
            replace: "NAME".into(),
            prefix: None,
            suffix: None,
            numbering: None,
        };
        let plan = plan(dir, &["name.txt".into()], &rules);
        assert_eq!(plan[0].new, "NAME.txt");
        assert_eq!(plan[0].status, RenameStatus::Ok);
    }

    #[test]
    fn numbering_widens_beyond_digits() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        let mut files = Vec::new();
        for index in 0..10 {
            let name = format!("f{index:02}.txt");
            touch(dir, &name);
            files.push(name);
        }
        let rules = RenameRules {
            find: None,
            replace: String::new(),
            prefix: None,
            suffix: Some("-".into()),
            numbering: Some(Numbering {
                start: 99,
                step: 1,
                digits: 2,
                at: NumberAt::Suffix,
            }),
        };
        let plan = plan(dir, &files, &rules);
        assert_eq!(plan[0].new, "f00-99.txt");
        assert_eq!(plan[1].new, "f01-100.txt");
    }

    #[test]
    fn scan_filters_extensions_and_cap() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        for name in ["a.txt", "b.log", "c.md", "d.TXT"] {
            touch(dir, name);
        }
        let names = scan(dir, &default_extensions(), 10).unwrap();
        assert_eq!(names, vec!["a.txt", "b.log", "d.TXT"]);
        let over = scan(dir, &default_extensions(), 2);
        assert!(matches!(over, Err(RenameError::TooMany(3))));
    }

    #[test]
    fn apply_and_undo_round_trip() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        for name in ["a.txt", "b.txt"] {
            touch(dir, name);
        }
        let pairs = vec![
            ("a.txt".to_string(), "a-01.txt".to_string()),
            ("b.txt".to_string(), "b-02.txt".to_string()),
        ];
        let applied = apply(dir, &pairs).unwrap();
        assert_eq!(applied.len(), 2);
        assert!(dir.join("a-01.txt").exists() && dir.join("b-02.txt").exists());
        undo(dir, &pairs).unwrap();
        assert!(dir.join("a.txt").exists() && dir.join("b.txt").exists());
        assert!(!dir.join("a-01.txt").exists());
    }

    #[test]
    fn apply_rolls_back_on_injected_failure() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        for name in ["a.txt", "b.txt", "c.txt"] {
            touch(dir, name);
        }
        let pairs = vec![
            ("a.txt".to_string(), "a1.txt".to_string()),
            ("b.txt".to_string(), "b1.txt".to_string()),
            ("c.txt".to_string(), "c1.txt".to_string()),
        ];
        let mut calls = 0usize;
        let result = apply_with(dir, &pairs, &mut |from, to| {
            calls += 1;
            if calls == 5 {
                return Err(io::Error::other("注入失败"));
            }
            fs::rename(from, to)
        });
        assert!(result.is_err());
        for name in ["a.txt", "b.txt", "c.txt"] {
            assert!(dir.join(name).exists(), "{name} 未回滚");
        }
        let leftovers: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|item| item.file_name().to_str().map(str::to_string))
            .filter(|name| name.contains("srt-ren"))
            .collect();
        assert!(leftovers.is_empty(), "存在临时残留：{leftovers:?}");
    }

    #[test]
    fn apply_case_only_rename() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        touch(dir, "name.txt");
        let pairs = vec![("name.txt".to_string(), "NAME.txt".to_string())];
        apply(dir, &pairs).unwrap();
        assert!(dir.join("NAME.txt").exists());
    }

    #[test]
    fn empty_pairs_is_noop() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path();
        touch(dir, "a.txt");
        let applied = apply(dir, &[]).unwrap();
        assert!(applied.is_empty());
    }
}
