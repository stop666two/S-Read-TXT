//! 系统集成：`.txt`/`.log` 关联候选、右键菜单、打开方式条目。
//!
//! # 设计
//!
//! - 用户级（`HKCU\Software\Classes`）**不需要管理员**：注册、注销、状态查询均直接进行；
//! - 全局（`HKLM\Software\Classes`）需要管理员：主进程以普通权限运行时，经
//!   `runas` 拉起自身助手模式（`--integration-write machine <txt> <log> <menu>`），
//!   助手仅写注册表后立即退出（不进入 Tauri/WebView2，避免提权与 WebView2 冲突）；
//! - **不抢占用户默认程序**：仅当扩展名默认值为空或已指向本应用时才写入；
//!   注销时仅当默认值仍指向本应用才还原（优先使用历史安装器遗留的 `_backup` 备份值）；
//! - 全新安装不再注册任何集成项（安装器零注册表写入）；一切由此模块按需开关；
//! - 所有键写入幂等，注销对不存在的键/值静默通过。
//!
//! 键布局（`<Classes>` = 用户级或全局的 `Software\Classes`）：
//!
//! ```text
//! <Classes>\SReadTXT.txt                       (默认)="S-Read-TXT 文本文档"
//!   \DefaultIcon                               (默认)="<exe>",0
//!   \shell\open\command                        (默认)="<exe>" "%1"
//! <Classes>\.txt\OpenWithProgids               SReadTXT.txt=""
//! <Classes>\.txt                               (默认) 空/Clear 时才写 SReadTXT.txt
//! <Classes>\Applications\<exe名>               FriendlyAppName/SupportedTypes/shell\open\command
//! <Classes>\SystemFileAssociations\.txt\shell\S-Read-TXT
//!                                              (默认)="用 S-Read-TXT 打开"、Icon、command
//! ```
//!
//! `.log` 与 `.txt` 结构相同（ProgID `SReadTXT.log`）。

use serde::{Deserialize, Serialize};

/// 作用域：用户级 / 全局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `HKCU\Software\Classes`（无需管理员）。
    User,
    /// `HKLM\Software\Classes`（需要管理员）。
    Machine,
}

impl Scope {
    /// 从命令参数解析作用域标识。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "machine" => Some(Self::Machine),
            _ => None,
        }
    }
}

/// 集成开关集合（同时用于状态查询与目标状态设置）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegOptions {
    /// `.txt` 关联候选。
    pub txt: bool,
    /// `.log` 关联候选。
    pub log: bool,
    /// 右键菜单「用 S-Read-TXT 打开」。
    pub context_menu: bool,
}

/// 双作用域状态（IPC 返回体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegStatus {
    /// 用户级（HKCU）。
    pub user: IntegOptions,
    /// 全局（HKLM）。
    pub machine: IntegOptions,
}

/// ProgID（`.txt`）。
pub(crate) const PROGID_TXT: &str = "SReadTXT.txt";
/// ProgID（`.log`）。
pub(crate) const PROGID_LOG: &str = "SReadTXT.log";
/// 类注册根（相对各 hive 根；`HKCU` 用户级 / `HKLM` 全局）。
pub(crate) const CLASSES_ROOT: &str = "Software\\Classes";
/// 右键菜单子键名（位于 `SystemFileAssociations\.<ext>\shell` 下）。
pub(crate) const MENU_KEY_NAME: &str = "S-Read-TXT";
/// 打开方式条目的 Applications 子键前缀。
pub(crate) const APPLICATIONS_PREFIX: &str = "Applications";
/// 关联描述（`.txt`）。
pub(crate) const DESC_TXT: &str = "S-Read-TXT 文本文档";
/// 关联描述（`.log`）。
pub(crate) const DESC_LOG: &str = "S-Read-TXT 日志文件";
/// 右键菜单显示文本。
pub(crate) const MENU_TEXT: &str = "用 S-Read-TXT 打开";
/// 打开方式列表中显示的应用名。
pub(crate) const APP_FRIENDLY_NAME: &str = "S-Read-TXT";
/// 历史安装器（FileAssociation.nsh）备份值后缀。
pub(crate) const BACKUP_SUFFIX: &str = "_backup";

/// `shell\open\command` 的命令行（右键菜单与关联共用）。
pub(crate) fn command_for(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

/// `DefaultIcon` 取值（图标索引 0）。
pub(crate) fn icon_for(exe: &str) -> String {
    format!("\"{exe}\",0")
}

/// 注册时默认值写入的决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefaultWrite {
    /// 写入我们的 ProgID（默认值为空或已是我们的）。
    Set,
    /// 保留既有默认值（属于其他程序，不抢占）。
    Keep,
}

/// 决策：注册 `.txt`/`.log` 时是否写入默认值。
///
/// 规则：空值或不存在的值 → 写入（无主扩展名顺手成为默认，体验更好）；
/// 已是我们 → 写入（幂等）；其他程序 → 保留（绝不抢占）。
pub(crate) fn decide_default_write(current: Option<&str>, ours: &str) -> DefaultWrite {
    match current.map(str::trim) {
        None | Some("") => DefaultWrite::Set,
        Some(value) if value.eq_ignore_ascii_case(ours) => DefaultWrite::Set,
        Some(_) => DefaultWrite::Keep,
    }
}

/// 注销时默认值的清理决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefaultCleanup {
    /// 还原为历史安装器备份值（`_backup`；仅非空时）。
    Restore,
    /// 清除默认值（我们写入且无备份）。
    Clear,
    /// 保留（默认值已不属于我们）。
    Keep,
}

/// 决策：注销 `.txt`/`.log` 时如何处理默认值。
///
/// 规则：默认值已不是我们 → 保留；是我们且存在非空备份 → 还原备份；
/// 是我们且无备份 → 清除（该值由我们写入，清除即恢复原状）。
pub(crate) fn decide_default_cleanup(
    current: Option<&str>,
    ours: &str,
    backup: Option<&str>,
) -> DefaultCleanup {
    match current.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case(ours) => match backup.map(str::trim) {
            Some(saved) if !saved.is_empty() => DefaultCleanup::Restore,
            _ => DefaultCleanup::Clear,
        },
        _ => DefaultCleanup::Keep,
    }
}

/// 助手模式参数解析结果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HelperParse {
    /// 非助手模式。
    NotHelper,
    /// 参数不完整或非法的助手模式（拒绝执行）。
    Invalid,
    /// 合法任务。
    Valid { scope: Scope, options: IntegOptions },
}

/// 助手模式开关。
pub(crate) const ARG_INTEGRATION: &str = "--integration-write";

/// 解析 `--integration-write <user|machine> <txt> <log> <menu>`（后三个为 `0`/`1`）。
///
/// 语义：`1` = 启用该项，`0` = 关闭该项；全 `0` 即完全注销。
pub(crate) fn parse_helper_args(args: &[String]) -> HelperParse {
    let Some(index) = args.iter().position(|arg| arg == ARG_INTEGRATION) else {
        return HelperParse::NotHelper;
    };
    let rest = &args[index + 1..];
    if rest.len() != 4 {
        return HelperParse::Invalid;
    }
    let Some(scope) = Scope::parse(&rest[0]) else {
        return HelperParse::Invalid;
    };
    let flags = rest[1..]
        .iter()
        .map(|value| match value.as_str() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        })
        .collect::<Option<Vec<bool>>>();
    let Some(flags) = flags else {
        return HelperParse::Invalid;
    };
    HelperParse::Valid {
        scope,
        options: IntegOptions {
            txt: flags[0],
            log: flags[1],
            context_menu: flags[2],
        },
    }
}

/// 当前进程可执行文件路径（供注册命令构造；失败回退空串——调用方会给出错误）。
pub(crate) fn current_exe_string() -> Result<String, String> {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .map_err(|err| format!("无法获取程序路径：{err}"))
}

#[cfg(windows)]
mod op {
    //! Windows 注册表读写（windows-sys 直调；无第三方依赖）。

    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW, RegDeleteValueW,
        RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    use super::Scope;

    /// 字符串 → 宽字符（含 NUL 结尾）。
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 作用域对应的根键。
    fn root(scope: Scope) -> HKEY {
        match scope {
            Scope::User => HKEY_CURRENT_USER,
            Scope::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    /// 已打开的注册表键（Drop 时关闭）。
    pub(super) struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    /// 打开键（读写；删除值需要写权限）；不存在返回 `None`。
    fn open(scope: Scope, path: &str) -> Option<Key> {
        let path = wide(path);
        let mut handle: HKEY = std::ptr::null_mut();
        let code = unsafe {
            RegOpenKeyExW(
                root(scope),
                path.as_ptr(),
                0,
                KEY_READ | KEY_SET_VALUE,
                &mut handle as *mut HKEY,
            )
        };
        if code == 0 {
            Some(Key(handle))
        } else {
            None
        }
    }

    /// 创建（或打开）键（读写）。
    fn create(scope: Scope, path: &str) -> Result<Key, String> {
        let display = path.to_string();
        let path = wide(path);
        let mut handle: HKEY = std::ptr::null_mut();
        let mut disposition = 0u32;
        let code = unsafe {
            RegCreateKeyExW(
                root(scope),
                path.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE | KEY_READ,
                std::ptr::null(),
                &mut handle as *mut HKEY,
                &mut disposition as *mut u32,
            )
        };
        if code == 0 {
            Ok(Key(handle))
        } else {
            Err(format!("创建注册表键失败（{display} 错误 {code}）"))
        }
    }

    /// 写入 REG_SZ 值（键不存在则创建）。
    pub(super) fn set_sz(scope: Scope, path: &str, name: &str, value: &str) -> Result<(), String> {
        let display = path.to_string();
        let key = create(scope, path)?;
        let name = wide(name);
        let data = wide(value);
        let bytes = data.len() * std::mem::size_of::<u16>();
        let code = unsafe {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                bytes as u32,
            )
        };
        if code == 0 {
            Ok(())
        } else {
            Err(format!("写入注册表值失败（{display} 错误 {code}）"))
        }
    }

    /// 读取 REG_SZ 值；键/值不存在或类型不符返回 `None`。
    pub(super) fn query_sz(scope: Scope, path: &str, name: &str) -> Option<String> {
        let key = open(scope, path)?;
        let name = wide(name);
        let mut value_type = 0u32;
        let mut size = 0u32;
        let code = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut value_type as *mut u32,
                std::ptr::null_mut(),
                &mut size as *mut u32,
            )
        };
        if code != 0 || value_type != REG_SZ || size == 0 {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let code = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut value_type as *mut u32,
                buffer.as_mut_ptr().cast(),
                &mut size as *mut u32,
            )
        };
        if code != 0 {
            return None;
        }
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    /// 删除值（键或值不存在时静默通过）。
    pub(super) fn delete_value(scope: Scope, path: &str, name: &str) {
        let Some(key) = open(scope, path) else {
            return;
        };
        let name = wide(name);
        unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
    }

    /// 删除键及其全部子键与值（不存在时静默通过）。
    pub(super) fn delete_key(scope: Scope, path: &str) {
        let path = wide(path);
        let _ = unsafe { RegDeleteTreeW(root(scope), path.as_ptr()) };
        let _ = unsafe { RegDeleteKeyW(root(scope), path.as_ptr()) };
    }
}

#[cfg(windows)]
mod win {
    //! 集成写入的 Windows 实现（HKCU 直写 / HKLM 由提权助手调用同一入口）。

    use super::op;
    use super::Scope;
    use super::{
        command_for, decide_default_cleanup, decide_default_write, icon_for, IntegOptions,
        APPLICATIONS_PREFIX, APP_FRIENDLY_NAME, BACKUP_SUFFIX, CLASSES_ROOT, DESC_LOG, DESC_TXT,
        MENU_KEY_NAME, MENU_TEXT, PROGID_LOG, PROGID_TXT,
    };

    /// 单个扩展名的注册/注销。
    fn apply_extension(
        scope: Scope,
        exe: &str,
        ext: &str,
        progid: &str,
        desc: &str,
        register: bool,
    ) -> Result<(), String> {
        let progid_key = format!("{CLASSES_ROOT}\\{progid}");
        let ext_key = format!("{CLASSES_ROOT}\\.{ext}");
        let backup_name = format!("{progid}{BACKUP_SUFFIX}");
        if register {
            op::set_sz(scope, &progid_key, "", desc)?;
            op::set_sz(
                scope,
                &format!("{progid_key}\\DefaultIcon"),
                "",
                &icon_for(exe),
            )?;
            op::set_sz(
                scope,
                &format!("{progid_key}\\shell\\open\\command"),
                "",
                &command_for(exe),
            )?;
            op::set_sz(scope, &format!("{ext_key}\\OpenWithProgids"), progid, "")?;
            let current = op::query_sz(scope, &ext_key, "");
            if decide_default_write(current.as_deref(), progid) == super::DefaultWrite::Set {
                op::set_sz(scope, &ext_key, "", progid)?;
            }
        } else {
            op::delete_key(scope, &progid_key);
            op::delete_value(scope, &format!("{ext_key}\\OpenWithProgids"), progid);
            let current = op::query_sz(scope, &ext_key, "");
            let backup = op::query_sz(scope, &ext_key, &backup_name);
            match decide_default_cleanup(current.as_deref(), progid, backup.as_deref()) {
                super::DefaultCleanup::Restore => {
                    if let Some(saved) = backup.filter(|value| !value.trim().is_empty()) {
                        op::set_sz(scope, &ext_key, "", saved.trim())?;
                    }
                    op::delete_value(scope, &ext_key, &backup_name);
                }
                super::DefaultCleanup::Clear => {
                    op::delete_value(scope, &ext_key, "");
                }
                super::DefaultCleanup::Keep => {}
            }
        }
        Ok(())
    }

    /// 右键菜单注册/注销。
    fn apply_context_menu(
        scope: Scope,
        exe: &str,
        ext: &str,
        register: bool,
    ) -> Result<(), String> {
        let base =
            format!("{CLASSES_ROOT}\\SystemFileAssociations\\.{ext}\\shell\\{MENU_KEY_NAME}");
        if register {
            op::set_sz(scope, &base, "", MENU_TEXT)?;
            op::set_sz(scope, &base, "Icon", &icon_for(exe))?;
            op::set_sz(scope, &base, "MultiSelectModel", "Single")?;
            op::set_sz(scope, &format!("{base}\\command"), "", &command_for(exe))?;
        } else {
            op::delete_key(scope, &base);
        }
        Ok(())
    }

    /// 「打开方式」列表条目注册/注销（`Applications\<exe 文件名>`）。
    fn apply_app_entry(
        scope: Scope,
        exe: &str,
        exe_name: &str,
        register: bool,
    ) -> Result<(), String> {
        let base = format!("{CLASSES_ROOT}\\{APPLICATIONS_PREFIX}\\{exe_name}");
        if register {
            op::set_sz(scope, &base, "FriendlyAppName", APP_FRIENDLY_NAME)?;
            op::set_sz(scope, &format!("{base}\\SupportedTypes"), ".txt", "")?;
            op::set_sz(scope, &format!("{base}\\SupportedTypes"), ".log", "")?;
            op::set_sz(
                scope,
                &format!("{base}\\shell\\open\\command"),
                "",
                &command_for(exe),
            )?;
        } else {
            op::delete_key(scope, &base);
        }
        Ok(())
    }

    /// 应用目标状态（幂等；逐项按需注册或注销）。
    pub fn set_state(scope: Scope, options: &IntegOptions) -> Result<(), String> {
        let exe = super::current_exe_string()?;
        let exe_name = std::path::Path::new(&exe)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "s-read-txt.exe".to_string());
        apply_extension(scope, &exe, "txt", PROGID_TXT, DESC_TXT, options.txt)?;
        apply_extension(scope, &exe, "log", PROGID_LOG, DESC_LOG, options.log)?;
        apply_context_menu(scope, &exe, "txt", options.context_menu)?;
        apply_context_menu(scope, &exe, "log", options.context_menu)?;
        apply_app_entry(scope, &exe, &exe_name, options.txt || options.log)?;
        Ok(())
    }

    /// 读取当前状态（命令值与本程序一致才算已注册）。
    pub fn read_state(scope: Scope) -> IntegOptions {
        let Ok(exe) = super::current_exe_string() else {
            return IntegOptions::default();
        };
        let expected = command_for(&exe);
        let registered = |progid: &str| {
            op::query_sz(
                scope,
                &format!("{CLASSES_ROOT}\\{progid}\\shell\\open\\command"),
                "",
            )
            .map(|value| value.eq_ignore_ascii_case(&expected))
            .unwrap_or(false)
        };
        let menu_registered = |ext: &str| {
            op::query_sz(
                scope,
                &format!(
                    "{CLASSES_ROOT}\\SystemFileAssociations\\.{ext}\\shell\\{MENU_KEY_NAME}\\command"
                ),
                "",
            )
            .map(|value| value.eq_ignore_ascii_case(&expected))
            .unwrap_or(false)
        };
        IntegOptions {
            txt: registered(PROGID_TXT),
            log: registered(PROGID_LOG),
            context_menu: menu_registered("txt") && menu_registered("log"),
        }
    }
}

/// 设置目标作用域状态（Windows 实现见 `win` 模块；其他平台不可用）。
#[cfg(windows)]
pub fn set_state(scope: Scope, options: &IntegOptions) -> Result<(), String> {
    win::set_state(scope, options)
}

/// 读取作用域状态。
#[cfg(windows)]
pub fn read_state(scope: Scope) -> IntegOptions {
    win::read_state(scope)
}

/// 非 Windows 桩：集成功能仅面向 Windows 发布。
#[cfg(not(windows))]
pub fn set_state(_scope: Scope, _options: &IntegOptions) -> Result<(), String> {
    Err("系统集成仅支持 Windows".to_string())
}

/// 非 Windows 桩。
#[cfg(not(windows))]
pub fn read_state(_scope: Scope) -> IntegOptions {
    IntegOptions::default()
}

/// 双作用域状态查询。
pub fn status() -> IntegStatus {
    IntegStatus {
        user: read_state(Scope::User),
        machine: read_state(Scope::Machine),
    }
}

/// 若当前进程以集成助手模式启动则执行写入并返回退出码。
///
/// 返回 `None` 表示普通启动；`Some(code)` 表示助手已完成（调用方应立即退出）。
/// 助手不初始化日志、不进入 Tauri/WebView2。
#[cfg(windows)]
pub fn maybe_run_helper_mode() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    match parse_helper_args(&args) {
        HelperParse::NotHelper => None,
        HelperParse::Invalid => {
            eprintln!("[s-read-txt] 集成助手参数不完整；用法：--integration-write <user|machine> <txt> <log> <menu>");
            Some(2)
        }
        HelperParse::Valid { scope, options } => Some(match set_state(scope, &options) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("[s-read-txt] 集成写入失败：{err}");
                1
            }
        }),
    }
}

/// 非 Windows 桩。
#[cfg(not(windows))]
pub fn maybe_run_helper_mode() -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 命令与图标格式：一律带引号，`%1` 占位。
    #[test]
    fn command_and_icon_format() {
        assert_eq!(command_for("C:\\a b\\x.exe"), "\"C:\\a b\\x.exe\" \"%1\"");
        assert_eq!(icon_for("C:\\a b\\x.exe"), "\"C:\\a b\\x.exe\",0");
    }

    /// 默认值写入决策：空/我们 → 写；其他程序 → 保留。
    #[test]
    fn default_write_rules() {
        assert_eq!(decide_default_write(None, "Ours.txt"), DefaultWrite::Set);
        assert_eq!(
            decide_default_write(Some("  "), "Ours.txt"),
            DefaultWrite::Set
        );
        assert_eq!(
            decide_default_write(Some("ours.TXT"), "ours.txt"),
            DefaultWrite::Set
        );
        assert_eq!(
            decide_default_write(Some("Notepad"), "Ours.txt"),
            DefaultWrite::Keep
        );
    }

    /// 默认值清理决策：非我们 → 保留；我们+备份 → 还原；我们无备份 → 清除。
    #[test]
    fn default_cleanup_rules() {
        assert_eq!(
            decide_default_cleanup(Some("Notepad"), "Ours.txt", Some("Old")),
            DefaultCleanup::Keep
        );
        assert_eq!(
            decide_default_cleanup(Some("Ours.txt"), "Ours.txt", Some("Old")),
            DefaultCleanup::Restore
        );
        assert_eq!(
            decide_default_cleanup(Some("Ours.txt"), "Ours.txt", None),
            DefaultCleanup::Clear
        );
        assert_eq!(
            decide_default_cleanup(Some("Ours.txt"), "Ours.txt", Some(" ")),
            DefaultCleanup::Clear
        );
    }

    /// 助手参数解析：非助手/非法/合法。
    #[test]
    fn helper_arg_parsing() {
        let plain = vec!["s-read-txt.exe".to_string()];
        assert_eq!(parse_helper_args(&plain), HelperParse::NotHelper);

        let short = vec!["s-read-txt.exe".to_string(), ARG_INTEGRATION.to_string()];
        assert_eq!(parse_helper_args(&short), HelperParse::Invalid);

        let bad_flag = vec![
            "s-read-txt.exe".to_string(),
            ARG_INTEGRATION.to_string(),
            "user".to_string(),
            "2".to_string(),
            "0".to_string(),
            "0".to_string(),
        ];
        assert_eq!(parse_helper_args(&bad_flag), HelperParse::Invalid);

        let valid = vec![
            "s-read-txt.exe".to_string(),
            ARG_INTEGRATION.to_string(),
            "machine".to_string(),
            "1".to_string(),
            "0".to_string(),
            "1".to_string(),
        ];
        assert_eq!(
            parse_helper_args(&valid),
            HelperParse::Valid {
                scope: Scope::Machine,
                options: IntegOptions {
                    txt: true,
                    log: false,
                    context_menu: true,
                },
            }
        );
    }

    /// 作用域解析。
    #[test]
    fn scope_parse() {
        assert_eq!(Scope::parse("user"), Some(Scope::User));
        assert_eq!(Scope::parse("machine"), Some(Scope::Machine));
        assert_eq!(Scope::parse("all"), None);
    }

    /// 手动机器级回归（写真实 HKCU 后清理；`cargo test -- --ignored` 手动运行）。
    #[test]
    #[ignore]
    fn manual_user_roundtrip() {
        let target = IntegOptions {
            txt: true,
            log: false,
            context_menu: false,
        };
        set_state(Scope::User, &target).expect("注册应成功");
        let state = read_state(Scope::User);
        assert!(state.txt, "读取状态应显示已注册：{state:?}");
        set_state(Scope::User, &IntegOptions::default()).expect("注销应成功");
        assert!(!read_state(Scope::User).txt, "注销后应恢复未注册");
    }
}
