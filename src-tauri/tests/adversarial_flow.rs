//! 对抗级集成测试（阶段 4b 扩测）：
//! 用户不会「老实使用」——畸形文件、换行族边角、代理对半区间、超大粘贴、
//! 外部状态异常（文件被删/被改）、编码不可表示、容量边界等。
//!
//! 全部经公开 API（AppState + 编辑命令路径）走真实文件与真实引擎。

use std::path::{Path, PathBuf};

use s_read_txt::app_state::{AppState, AppStateError};
use s_read_txt::settings::model::AppSettings;
use s_read_txt::textfile::editing::edit_doc::{EditError, EditOp};
use s_read_txt::textfile::editing::save::SaveError;
use s_read_txt::textfile::encoding::FileEncoding;

/// 写测试文件（原始字节）。
fn write_bytes(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("写测试文件失败");
    path
}

/// 打开文件并返回标签 id（测试内均为新标签）。
fn open_tab(state: &mut AppState, settings: &AppSettings, path: &Path) -> u64 {
    let (info, reused) = state.open_file(path, settings).expect("打开失败");
    assert!(!reused, "测试用例内不应复用标签");
    info.tab_id
}

/// 取若干行文本（便于断言）。
fn row_texts(state: &AppState, tab_id: u64, start: u64, count: u32) -> Vec<String> {
    state
        .rows(tab_id, start, count)
        .expect("取行失败")
        .rows
        .into_iter()
        .map(|row| row.text)
        .collect()
}

/// 仅 CR 换行的文件：读取、编辑、保存后换行族保持不变。
#[test]
fn cr_only_roundtrip() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "cr.txt", b"a\rb\r");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);

    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 2);
    assert_eq!(row_texts(&state, tab, 0, 2), vec!["a", "b"]);

    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 1,
                utf16: 1,
                text: "X".to_string(),
            }],
        )
        .expect("插入失败");
    state.save_edit(tab, None, false, false).expect("保存失败");
    assert_eq!(std::fs::read(&path).expect("读回失败"), b"a\rbX\r");
}

/// 混合换行族：逐级行首退格合并（Delete 跨行），再全部撤销回原文。
#[test]
fn mixed_newlines_merge_then_undo_all() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "mix.txt", b"a\r\nb\nc\rd");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 4);

    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    // 模拟行首退格：删除前一行行尾与当前行行首之间的换行单元
    for prev_len in [1, 2, 3] {
        state
            .apply_edit_ops(
                tab,
                &[EditOp::Delete {
                    start_row: 0,
                    start_utf16: prev_len,
                    end_row: 1,
                    end_utf16: 0,
                }],
            )
            .expect("合并失败");
    }
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 1);
    assert_eq!(row_texts(&state, tab, 0, 1), vec!["abcd"]);

    for _ in 0..3 {
        assert!(state.undo_edit(tab).expect("撤销失败").is_some());
    }
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 4);
    assert_eq!(
        row_texts(&state, tab, 0, 4),
        vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string()
        ]
    );
}

/// 仅含 BOM 的空文件：可进入编辑、插入、保存且 BOM 保留。
#[test]
fn bom_only_insert_save() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "bom.txt", &[0xEF, 0xBB, 0xBF]);
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);

    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 1);
    assert_eq!(row_texts(&state, tab, 0, 1), vec![""]);

    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "x".to_string(),
            }],
        )
        .expect("插入失败");
    state.save_edit(tab, None, false, false).expect("保存失败");
    assert_eq!(
        std::fs::read(&path).expect("读回失败"),
        vec![0xEF, 0xBB, 0xBF, b'x']
    );
}

/// 含 NUL 字节的文本：读取、编辑、保存字节级往返一致。
///
/// 注意：测试文件名避免 Windows 保留设备名（NUL/CON/PRN/AUX/COM1-9/LPT1-9，
/// 含扩展名同样命中设备），否则会打开设备而非文件。
#[test]
fn null_bytes_roundtrip() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "has-null.txt", b"a\0b\n");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);

    assert_eq!(row_texts(&state, tab, 0, 1), vec!["a\0b"]);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 3,
                text: "Z".to_string(),
            }],
        )
        .expect("插入失败");
    state.save_edit(tab, None, false, false).expect("保存失败");
    assert_eq!(std::fs::read(&path).expect("读回失败"), b"a\0bZ\n");
}

/// 粘贴含 CRLF 的文本：行数按换行族正确拆分。
#[test]
fn pasted_crlf_creates_rows() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "paste.txt", b"abc\ndef\n");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);

    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "x\r\ny".to_string(),
            }],
        )
        .expect("插入失败");
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 3);
    assert_eq!(
        row_texts(&state, tab, 0, 3),
        vec!["x".to_string(), "yabc".to_string(), "def".to_string()]
    );
}

/// 1 万字符大粘贴：行数与磁盘字节精确一致。
#[test]
fn large_paste_roundtrip() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "big.txt", b"abc");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);

    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    let text = "Ж".repeat(10_000);
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: text.clone(),
            }],
        )
        .expect("插入失败");
    // 20 003 字节单行 → 显示分段（8KB 粒度）= 3 段：8192 + 8192 + 3619
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 3);
    // 分段展示不改变保存内容
    state.save_edit(tab, None, false, false).expect("保存失败");
    assert_eq!(
        std::fs::read(&path).expect("读回失败"),
        format!("{text}abc").as_bytes()
    );
}

/// 文档末尾连续追加 100 次，再撤销/重做各 100 次：计数与脏态精确。
#[test]
fn append_100_undo_redo() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "tail.txt", b"base");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");

    let mut len = 4u64;
    for _ in 0..100 {
        state
            .apply_edit_ops(
                tab,
                &[EditOp::Insert {
                    row: 0,
                    utf16: len,
                    text: "x".to_string(),
                }],
            )
            .expect("追加失败");
        len += 1;
    }
    assert_eq!(row_texts(&state, tab, 0, 1)[0].len(), 104);
    assert!(state.tab_info(tab).expect("标签缺失").dirty);

    for _ in 0..100 {
        assert!(state.undo_edit(tab).expect("撤销失败").is_some());
    }
    assert_eq!(row_texts(&state, tab, 0, 1), vec!["base"]);
    assert!(
        !state.tab_info(tab).expect("标签缺失").dirty,
        "撤销回打开时的状态应视为干净"
    );

    for _ in 0..100 {
        assert!(state.redo_edit(tab).expect("重做失败").is_some());
    }
    assert_eq!(row_texts(&state, tab, 0, 1)[0].len(), 104);
}

/// 半代理对删除区间：吸附为空操作（不 panic、不产生半个字符）；整对删除可用。
#[test]
fn mid_surrogate_delete_is_safe_noop() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "emoji.txt", "a😀b".as_bytes());
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");

    let half = state.apply_edit_ops(
        tab,
        &[EditOp::Delete {
            start_row: 0,
            start_utf16: 1,
            end_row: 0,
            end_utf16: 2,
        }],
    );
    assert!(half.is_ok(), "半代理对区间不应报错");
    assert_eq!(
        row_texts(&state, tab, 0, 1),
        vec!["a😀b"],
        "半代理对区间应吸附为空操作"
    );

    state
        .apply_edit_ops(
            tab,
            &[EditOp::Delete {
                start_row: 0,
                start_utf16: 1,
                end_row: 0,
                end_utf16: 3,
            }],
        )
        .expect("整对删除失败");
    assert_eq!(row_texts(&state, tab, 0, 1), vec!["ab"]);
}

/// 跨稀疏检查点（512 行）的大区间替换：行映射与撤销正确。
#[test]
fn replace_across_checkpoint_boundary() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let content: String = (0..600).map(|index| format!("line-{index}\n")).collect();
    let path = write_bytes(dir.path(), "big-rows.txt", content.as_bytes());
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 600);

    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Replace {
                start_row: 500,
                start_utf16: 0,
                end_row: 520,
                end_utf16: 0,
                text: "R".to_string(),
            }],
        )
        .expect("替换失败");
    // 语义：删除 [line-500 行首, line-520 行首)（含 20 个换行的 20 行）
    // 并原地插入 "R"（无换行）→ "R" 与 line-520 并入同一行：
    // 行数 = 600 - 20 个换行 = 580；第 500 行为 "Rline-520"。
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 580);
    assert_eq!(
        row_texts(&state, tab, 499, 3),
        vec![
            "line-499".to_string(),
            "Rline-520".to_string(),
            "line-521".to_string()
        ]
    );

    assert!(state.undo_edit(tab).expect("撤销失败").is_some());
    assert_eq!(state.tab_info(tab).expect("标签缺失").rows_total, 600);
    assert_eq!(
        row_texts(&state, tab, 500, 2),
        vec!["line-500".to_string(), "line-501".to_string()]
    );
}

/// 另存为不存在的目录：失败且原子（原文件与脏态不变）。
#[test]
fn save_as_missing_dir_fails_atomically() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "keep.txt", b"abc");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "Z".to_string(),
            }],
        )
        .expect("插入失败");

    let missing = dir.path().join("no-such-dir").join("x.txt");
    let result = state.save_edit_as(tab, &missing, None, false, &settings);
    assert!(result.is_err(), "不存在的目标目录应失败");
    assert!(
        state.tab_info(tab).expect("标签缺失").dirty,
        "失败后应保持脏态"
    );
    assert_eq!(std::fs::read(&path).expect("读回失败"), b"abc");
}

/// Big5 无法表示 emoji：保存失败、磁盘原文件不变、脏态保持。
///
/// 说明：GB18030 是全 Unicode 编码（可表示 emoji），因此用 Big5 验证
/// 「不可表示字符」路径。
#[test]
fn big5_unrepresentable_leaves_file_intact() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "gb.txt", b"abc");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "😀".to_string(),
            }],
        )
        .expect("插入失败");

    let result = state.save_edit(tab, Some(FileEncoding::Big5), false, false);
    assert!(
        matches!(
            result,
            Err(AppStateError::Save(SaveError::Unrepresentable { .. }))
        ),
        "应为不可表示字符错误：{result:?}"
    );
    assert_eq!(std::fs::read(&path).expect("读回失败"), b"abc");
    assert!(state.tab_info(tab).expect("标签缺失").dirty);
}

/// 打开中的文件被外部删除后再保存：按「当前无文件」处理并重建（行为文档化）。
#[test]
fn external_delete_then_save_recreates() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "gone.txt", b"abc");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "Z".to_string(),
            }],
        )
        .expect("插入失败");

    std::fs::remove_file(&path).expect("外部删除失败");
    state
        .save_edit(tab, None, false, false)
        .expect("删除后保存应重建文件");
    assert_eq!(std::fs::read(&path).expect("读回失败"), b"Zabc");
}

/// 未保存修改阻止的能力在切回干净后恢复；编码切换会重建编辑文档。
#[test]
fn clean_encoding_switch_drops_edit_doc() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "enc.txt", "中文\n".as_bytes());
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");

    let info = state
        .set_encoding(tab, Some(FileEncoding::Gb18030))
        .expect("干净时允许切换编码");
    assert!(!info.editing);
    assert!(!info.dirty);
    assert_eq!(info.encoding, "GB18030");

    let edit = state.toggle_edit(tab, &settings).expect("再次进入编辑失败");
    assert!(edit.editing);
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "X".to_string(),
            }],
        )
        .expect("按新编码编辑失败");
    assert!(state.tab_info(tab).expect("标签缺失").dirty);
}

/// 标签容量边界：达到上限拒绝、关闭中间标签后可再开、活动标签正确。
#[test]
fn max_tabs_boundary_open_close_middle() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let paths: Vec<PathBuf> = ["a", "b", "c", "d"]
        .iter()
        .map(|name| write_bytes(dir.path(), &format!("{name}.txt"), b"x\n"))
        .collect();
    let mut state = AppState::new();
    let mut settings = AppSettings::default();
    settings.max_tabs = 3;

    let first = open_tab(&mut state, &settings, &paths[0]);
    let second = open_tab(&mut state, &settings, &paths[1]);
    open_tab(&mut state, &settings, &paths[2]);
    assert!(matches!(
        state.open_file(&paths[3], &settings),
        Err(AppStateError::MaxTabs { limit: 3 })
    ));

    assert!(state.close(second));
    let fourth = open_tab(&mut state, &settings, &paths[3]);
    assert_eq!(state.tabs_info().len(), 3);
    assert_eq!(state.active_tab(), Some(fourth));
    assert!(state.tab_info(first).is_some());
}

/// 超大 UTF-16 偏移（前端哨兵值越界下发）：引擎防御性拒绝且文档不变。
///
/// 背景：前端「全选」曾使用 MAX_SAFE_INTEGER 哨兵，未钳制就下发引擎被拒，
/// 导致全选后的输入/删除静默失败；引擎拒绝是正确防线，前端必须钳制。
#[test]
fn oversized_utf16_is_rejected_safely() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_bytes(dir.path(), "sentinel.txt", b"alpha\nbeta\n");
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let tab = open_tab(&mut state, &settings, &path);
    state.toggle_edit(tab, &settings).expect("进入编辑失败");

    let result = state.apply_edit_ops(
        tab,
        &[EditOp::Insert {
            row: 0,
            utf16: u64::MAX,
            text: "X".to_string(),
        }],
    );
    assert!(
        matches!(
            result,
            Err(AppStateError::Edit(EditError::Utf16OutOfRange { .. }))
        ),
        "越界偏移应被拒绝：{result:?}"
    );
    assert_eq!(
        row_texts(&state, tab, 0, 2),
        vec!["alpha".to_string(), "beta".to_string()],
        "拒绝后文档不得变化"
    );

    // 正常偏移仍可用
    state
        .apply_edit_ops(
            tab,
            &[EditOp::Insert {
                row: 0,
                utf16: 5,
                text: "!".to_string(),
            }],
        )
        .expect("正常偏移插入失败");
    assert_eq!(row_texts(&state, tab, 0, 1), vec!["alpha!"]);
}
