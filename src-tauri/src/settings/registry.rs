//! 设置注册表：全部设置项的元数据（稳定 id、分组、中文标签、类型与范围）。
//!
//! 用途（单一事实源）：
//! - `get_settings_registry` 命令：设置界面据此动态生成控件、分组与搜索索引；
//! - `bundle` 导入校验：按注册表做严格类型 / 范围 / 枚举检查并检测未知字段；
//! - `reset` 重置：按注册表解析「单项 / 分组」作用域。
//!
//! 约定：
//! - `id` 为点分路径（与配置文件 JSON 结构一致，如 `app.history.maxEntries`）；
//! - 数值范围引用 [`crate::settings::defaults`] 常量，避免与模型归一逻辑漂移；
//! - 枚举取值与模型 serde 序列化名一致（有测试守护，改枚举名会立即失败）。

use serde::Serialize;
use serde_json::Value;

use crate::settings::defaults;

/// 设置项类型（序列化给前端；标签字段 `type` 决定控件形态）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SettingKind {
    /// 数值（闭区间；`integer` 表示仅接受整数）
    Number {
        /// 最小值（含）
        min: f64,
        /// 最大值（含）
        max: f64,
        /// 是否仅接受整数
        integer: bool,
    },
    /// 布尔开关
    Bool,
    /// 枚举（取值列表与模型 serde 名一致）
    Enum {
        /// 允许的取值
        values: &'static [&'static str],
    },
    /// 文本（按 `char` 计数的最大长度）
    Text {
        /// 最大字符数
        max_len: u32,
    },
    /// 快捷键绑定表（动作 id → 组合键；动作白名单见 `defaults::is_known_action`）
    Shortcuts,
}

/// 单个设置项的元数据。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingSpec {
    /// 稳定点分 id（配置文件 JSON 路径；前端据此解析语言包键 `setting.<id>`）
    pub id: &'static str,
    /// 分组（设置界面分组与重置作用域；语言包键 `settingGroup.<group>`）
    pub group: &'static str,
    /// 类型与范围
    pub kind: SettingKind,
}

/// 全部设置项（顺序即界面默认展示顺序：主配置 → 阅读 → 快捷键）。
pub const SPECS: &[SettingSpec] = &[
    // ---------- settings.json / 常规 ----------
    SettingSpec {
        id: "app.logLevel",
        group: "app.basic",
        kind: SettingKind::Enum {
            values: &["error", "warn", "info", "debug"],
        },
    },
    SettingSpec {
        id: "app.locale",
        group: "app.basic",
        kind: SettingKind::Enum {
            values: &["zh-CN", "en"],
        },
    },
    SettingSpec {
        id: "app.maxFileSizeMB",
        group: "app.basic",
        kind: SettingKind::Number {
            min: defaults::MAX_FILE_SIZE_MB_RANGE.0 as f64,
            max: defaults::MAX_FILE_SIZE_MB_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "app.hardLimitMB",
        group: "app.basic",
        kind: SettingKind::Number {
            min: defaults::HARD_LIMIT_MB_RANGE.0 as f64,
            max: defaults::HARD_LIMIT_MB_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "app.maxTabs",
        group: "app.basic",
        kind: SettingKind::Number {
            min: defaults::MAX_TABS_RANGE.0 as f64,
            max: defaults::MAX_TABS_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "app.saveBackupEnabled",
        group: "app.basic",
        kind: SettingKind::Bool,
    },
    SettingSpec {
        id: "app.showOnboarding",
        group: "app.basic",
        kind: SettingKind::Bool,
    },
    // ---------- settings.json / 历史 ----------
    SettingSpec {
        id: "app.history.maxEntries",
        group: "app.history",
        kind: SettingKind::Number {
            min: defaults::HISTORY_MAX_ENTRIES_RANGE.0 as f64,
            max: defaults::HISTORY_MAX_ENTRIES_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "app.history.retentionDays",
        group: "app.history",
        kind: SettingKind::Number {
            min: defaults::HISTORY_RETENTION_DAYS_RANGE.0 as f64,
            max: defaults::HISTORY_RETENTION_DAYS_RANGE.1 as f64,
            integer: true,
        },
    },
    // ---------- settings.json / 启动行为 ----------
    SettingSpec {
        id: "app.startup.restoreSession",
        group: "app.startup",
        kind: SettingKind::Bool,
    },
    SettingSpec {
        id: "app.startup.restoreWindow",
        group: "app.startup",
        kind: SettingKind::Bool,
    },
    // ---------- reader.json / 主题 ----------
    SettingSpec {
        id: "reader.theme",
        group: "reader.basic",
        // 主题 id 为开放集合（内置 6 套 + 用户主题 data/themes/<id>.json），
        // 仅做长度约束；有效性由 theme 模块在解析/导入时强校验。
        kind: SettingKind::Text { max_len: 64 },
    },
    SettingSpec {
        id: "reader.themeAnimEnabled",
        group: "reader.basic",
        kind: SettingKind::Bool,
    },
    SettingSpec {
        id: "reader.themeAnimMs",
        group: "reader.basic",
        kind: SettingKind::Number {
            min: defaults::THEME_ANIM_MS_RANGE.0 as f64,
            max: defaults::THEME_ANIM_MS_RANGE.1 as f64,
            integer: true,
        },
    },
    // ---------- reader.json / 排版 ----------
    SettingSpec {
        id: "reader.typography.fontFamily",
        group: "reader.typography",
        kind: SettingKind::Text { max_len: 200 },
    },
    SettingSpec {
        id: "reader.typography.fontSize",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::FONT_SIZE_RANGE.0 as f64,
            max: defaults::FONT_SIZE_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "reader.typography.lineHeight",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::LINE_HEIGHT_RANGE.0 as f64,
            max: defaults::LINE_HEIGHT_RANGE.1 as f64,
            integer: false,
        },
    },
    SettingSpec {
        id: "reader.typography.contentWidth",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::CONTENT_WIDTH_RANGE.0 as f64,
            max: defaults::CONTENT_WIDTH_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "reader.typography.pagePadding",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::PAGE_PADDING_RANGE.0 as f64,
            max: defaults::PAGE_PADDING_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "reader.typography.pagePaddingY",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::PAGE_PADDING_Y_RANGE.0 as f64,
            max: defaults::PAGE_PADDING_Y_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "reader.typography.paragraphSpacing",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::PARAGRAPH_SPACING_RANGE.0 as f64,
            max: defaults::PARAGRAPH_SPACING_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "reader.typography.firstLineIndent",
        group: "reader.typography",
        kind: SettingKind::Number {
            min: defaults::FIRST_LINE_INDENT_RANGE.0 as f64,
            max: defaults::FIRST_LINE_INDENT_RANGE.1 as f64,
            integer: true,
        },
    },
    SettingSpec {
        id: "reader.typography.textAlign",
        group: "reader.typography",
        kind: SettingKind::Enum {
            values: &["left", "justify"],
        },
    },
    SettingSpec {
        id: "reader.typography.smoothScroll",
        group: "reader.typography",
        kind: SettingKind::Bool,
    },
    // ---------- reader.json / 状态栏 ----------
    SettingSpec {
        id: "reader.statusBar.showFileName",
        group: "reader.statusBar",
        kind: SettingKind::Bool,
    },
    SettingSpec {
        id: "reader.statusBar.showPercent",
        group: "reader.statusBar",
        kind: SettingKind::Bool,
    },
    SettingSpec {
        id: "reader.statusBar.showSize",
        group: "reader.statusBar",
        kind: SettingKind::Bool,
    },
    SettingSpec {
        id: "reader.statusBar.showEncoding",
        group: "reader.statusBar",
        kind: SettingKind::Bool,
    },
    // ---------- shortcuts.json ----------
    SettingSpec {
        id: "shortcuts.bindings",
        group: "shortcuts",
        kind: SettingKind::Shortcuts,
    },
];

/// 按 id 查设置项。
pub fn spec_by_id(id: &str) -> Option<&'static SettingSpec> {
    SPECS.iter().find(|spec| spec.id == id)
}

/// 按分组枚举设置项（分组不存在时为空迭代器，调用方自行判断）。
pub fn specs_in_group(group: &str) -> impl Iterator<Item = &'static SettingSpec> + '_ {
    SPECS.iter().filter(move |spec| spec.group == group)
}

/// 按点分路径取值（纯对象路径；任一段缺失或类型不符返回 `None`）。
pub fn navigate<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.as_object()?.get(segment)?;
    }
    Some(current)
}

/// 按点分路径替换已有值（`segments` 为相对路径；路径必须已存在，否则返回 `false`）。
pub fn set_at(value: &mut Value, segments: &[&str], new_value: Value) -> bool {
    let Some((last, parents)) = segments.split_last() else {
        return false;
    };
    let mut current = value;
    for segment in parents {
        let Some(next) = current
            .as_object_mut()
            .and_then(|obj| obj.get_mut(*segment))
        else {
            return false;
        };
        current = next;
    }
    let Some(obj) = current.as_object_mut() else {
        return false;
    };
    let Some(slot) = obj.get_mut(*last) else {
        return false;
    };
    *slot = new_value;
    true
}

/// 严格校验单个设置项取值（导入用；错误消息含字段 id 与中文原因）。
pub fn validate_value(spec: &SettingSpec, value: &Value) -> Result<(), String> {
    match spec.kind {
        SettingKind::Number { min, max, integer } => {
            if integer {
                let raw = value
                    .as_u64()
                    .ok_or_else(|| format!("{}：应为非负整数", spec.id))?;
                let number = raw as f64;
                if number < min || number > max {
                    return Err(format!(
                        "{}：数值超出允许范围 {min}–{max}（实际 {number}）",
                        spec.id
                    ));
                }
            } else {
                let number = value
                    .as_f64()
                    .ok_or_else(|| format!("{}：应为数字", spec.id))?;
                if number < min || number > max {
                    return Err(format!(
                        "{}：数值超出允许范围 {min}–{max}（实际 {number}）",
                        spec.id
                    ));
                }
            }
            Ok(())
        }
        SettingKind::Bool => value
            .as_bool()
            .map(|_| ())
            .ok_or_else(|| format!("{}：应为 true/false", spec.id)),
        SettingKind::Enum { values } => {
            let raw = value
                .as_str()
                .ok_or_else(|| format!("{}：应为字符串", spec.id))?;
            if values.contains(&raw) {
                Ok(())
            } else {
                Err(format!(
                    "{}：非法取值「{raw}」（允许：{}）",
                    spec.id,
                    values.join("/")
                ))
            }
        }
        SettingKind::Text { max_len } => {
            let raw = value
                .as_str()
                .ok_or_else(|| format!("{}：应为字符串", spec.id))?;
            if raw.chars().count() > max_len as usize {
                return Err(format!("{}：文本过长（上限 {max_len} 字符）", spec.id));
            }
            Ok(())
        }
        SettingKind::Shortcuts => {
            let map = value
                .as_object()
                .ok_or_else(|| format!("{}：应为对象", spec.id))?;
            for (action, combo) in map {
                if !defaults::is_known_action(action) {
                    return Err(format!("{}：未知动作「{action}」", spec.id));
                }
                let combo = combo
                    .as_str()
                    .ok_or_else(|| format!("{}.{action}：应为字符串", spec.id))?;
                if combo.trim().is_empty() {
                    return Err(format!("{}.{action}：绑定不能为空", spec.id));
                }
                if combo.chars().count() > 64 {
                    return Err(format!("{}.{action}：绑定过长", spec.id));
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::model::{Language, LogLevel};
    use crate::settings::reader::TextAlign;

    /// id 唯一、前缀合法、分组/标签非空、数值范围不倒挂。
    #[test]
    fn spec_ids_are_unique_and_wellformed() {
        let mut ids = std::collections::BTreeSet::new();
        for spec in SPECS {
            assert!(ids.insert(spec.id), "重复 id：{}", spec.id);
            assert!(
                spec.id.starts_with("app.")
                    || spec.id.starts_with("reader.")
                    || spec.id == "shortcuts.bindings",
                "非法前缀：{}",
                spec.id
            );
            assert!(!spec.group.is_empty(), "空分组：{}", spec.id);
            if let SettingKind::Number { min, max, .. } = spec.kind {
                assert!(min <= max, "范围倒挂：{}", spec.id);
            }
        }
    }

    /// 查询助手：按 id 与按分组。
    #[test]
    fn lookup_helpers_work() {
        assert!(matches!(
            spec_by_id("app.maxTabs").expect("存在").kind,
            SettingKind::Number { integer: true, .. }
        ));
        assert!(spec_by_id("app.notExist").is_none());
        assert_eq!(specs_in_group("reader.typography").count(), 10);
        assert_eq!(specs_in_group("no.such.group").count(), 0);
    }

    /// 枚举取值必须与模型 serde 序列化名一致（改枚举名会立即失败）。
    #[test]
    fn enum_values_match_model_serialization() {
        fn names<T: Serialize>(variants: &[T]) -> Vec<String> {
            variants
                .iter()
                .map(|v| {
                    serde_json::to_value(v)
                        .expect("序列化")
                        .as_str()
                        .expect("字符串")
                        .to_string()
                })
                .collect()
        }
        let log = names(&[
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
        ]);
        let language = names(&[Language::ZhCn, Language::En]);
        let align = names(&[TextAlign::Left, TextAlign::Justify]);
        for (id, expected) in [
            ("app.logLevel", log),
            ("app.locale", language),
            ("reader.typography.textAlign", align),
        ] {
            let SettingKind::Enum { values } = spec_by_id(id).expect("存在").kind else {
                panic!("{id} 应为枚举");
            };
            let actual: Vec<String> = values.iter().map(|v| v.to_string()).collect();
            assert_eq!(actual, expected, "{id} 枚举取值漂移");
        }
    }

    /// 校验：越界 / 类型错误 / 未知动作均拒绝并带字段路径。
    #[test]
    fn validate_value_rejects_bad_values() {
        let max_tabs = spec_by_id("app.maxTabs").expect("存在");
        assert!(validate_value(max_tabs, &serde_json::json!(30)).is_ok());
        let err = validate_value(max_tabs, &serde_json::json!(9999)).expect_err("越界应拒绝");
        assert!(err.contains("app.maxTabs") && err.contains("范围"), "{err}");
        assert!(validate_value(max_tabs, &serde_json::json!("30")).is_err());
        let theme = spec_by_id("reader.theme").expect("存在");
        assert!(validate_value(theme, &serde_json::json!("dark")).is_ok());
        assert!(validate_value(theme, &serde_json::json!(42)).is_err());
        let shortcuts = spec_by_id("shortcuts.bindings").expect("存在");
        assert!(validate_value(shortcuts, &serde_json::json!({})).is_ok());
        assert!(validate_value(shortcuts, &serde_json::json!({"unknown": "Ctrl+A"})).is_err());
        assert!(validate_value(shortcuts, &serde_json::json!({"openFile": "  "})).is_err());
    }

    /// 路径读写助手往返。
    #[test]
    fn navigate_and_set_at_roundtrip() {
        let mut value = serde_json::json!({"a": {"b": {"c": 1}}});
        assert_eq!(navigate(&value, "a.b.c"), Some(&serde_json::json!(1)));
        assert_eq!(navigate(&value, "a.x.c"), None);
        assert!(set_at(&mut value, &["a", "b", "c"], serde_json::json!(2)));
        assert_eq!(navigate(&value, "a.b.c"), Some(&serde_json::json!(2)));
        assert!(!set_at(
            &mut value,
            &["a", "b", "missing"],
            serde_json::json!(1)
        ));
    }

    /// 完备性：默认配置的每个叶子字段都必须登记在注册表（`schemaVersion` 与
    /// `shortcuts.bindings.<动作>` 按白名单豁免）；每个 `app.`/`reader.` 注册项
    /// 都必须能导航到默认配置中的真实字段——双向防漂移。
    #[test]
    fn registry_covers_every_default_field() {
        use crate::settings::model::AppSettings;
        use crate::settings::reader::ReaderSettings;
        use crate::settings::shortcuts::ShortcutSettings;

        fn leaves(value: &Value, prefix: &str, out: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    for (key, child) in map {
                        leaves(child, &format!("{prefix}.{key}"), out);
                    }
                }
                _ => out.push(prefix.to_string()),
            }
        }
        let app_tree = serde_json::to_value(AppSettings::default()).expect("app 树");
        let reader_tree = serde_json::to_value(ReaderSettings::default()).expect("reader 树");
        let shortcuts_tree = serde_json::to_value(ShortcutSettings {
            schema_version: crate::settings::defaults::SCHEMA_VERSION,
            bindings: crate::settings::defaults::default_bindings(),
        })
        .expect("shortcuts 树");
        for (section, tree) in [
            ("app", &app_tree),
            ("reader", &reader_tree),
            ("shortcuts", &shortcuts_tree),
        ] {
            let mut paths = Vec::new();
            leaves(tree, section, &mut paths);
            for path in paths {
                if path.ends_with(".schemaVersion") || path.starts_with("shortcuts.bindings.") {
                    continue;
                }
                assert!(spec_by_id(&path).is_some(), "字段 {path} 未登记注册表");
            }
        }
        for spec in SPECS {
            match spec.id.split_once('.') {
                Some(("app", rest)) => {
                    assert!(
                        navigate(&app_tree, rest).is_some(),
                        "注册项 {} 无法导航",
                        spec.id
                    );
                }
                Some(("reader", rest)) => {
                    assert!(
                        navigate(&reader_tree, rest).is_some(),
                        "注册项 {} 无法导航",
                        spec.id
                    );
                }
                _ => {}
            }
        }
    }

    /// 边界校验：整数项拒绝浮点；文本超长；快捷键空值/超长/类型；枚举未知值；
    /// 布尔类型错误；边界值本身应通过。
    #[test]
    fn validate_value_boundary_cases() {
        let spec = |id: &str| spec_by_id(id).expect(id);
        let err =
            validate_value(spec("app.maxTabs"), &serde_json::json!(1.5)).expect_err("浮点应拒绝");
        assert!(err.contains("非负整数"), "{err}");

        let long = "字".repeat(201);
        let err = validate_value(
            spec("reader.typography.fontFamily"),
            &serde_json::json!(long),
        )
        .expect_err("超长应拒绝");
        assert!(err.contains("过长"), "{err}");

        let long_combo = format!("Ctrl+{}", "A".repeat(70));
        let err = validate_value(
            spec("shortcuts.bindings"),
            &serde_json::json!({ "openFile": long_combo }),
        )
        .expect_err("绑定过长应拒绝");
        assert!(err.contains("过长"), "{err}");

        let err =
            validate_value(spec("app.locale"), &serde_json::json!("fr")).expect_err("枚举应拒绝");
        assert!(err.contains("非法取值"), "{err}");

        let err = validate_value(spec("reader.statusBar.showSize"), &serde_json::json!("yes"))
            .expect_err("类型应拒绝");
        assert!(err.contains("true/false"), "{err}");

        assert!(validate_value(spec("app.maxTabs"), &serde_json::json!(200)).is_ok());
        assert!(validate_value(
            spec("reader.typography.fontFamily"),
            &serde_json::json!("微软雅黑")
        )
        .is_ok());
        assert!(validate_value(
            spec("shortcuts.bindings"),
            &serde_json::json!({"openFile": "Ctrl+O"})
        )
        .is_ok());
    }
}
