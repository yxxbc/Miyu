//! 路径测试共用的 fixture。

use crate::paths::*;

pub(super) fn test_layouts(root: &Path) -> (LegacyLayout, Layout) {
    (
        LegacyLayout {
            config_dir: root.join("legacy/config"),
            data_dir: root.join("legacy/data"),
            cache_dir: root.join("legacy/cache"),
            state_dir: root.join("legacy/state"),
            documents_dir: root.join("Documents/Miyu"),
            pictures_dirs: vec![root.join("Pictures/gqy"), root.join("Pictures/Miyu")],
        },
        Layout {
            root_dir: root.join(".gqy"),
            config_dir: root.join(".gqy/config"),
            data_dir: root.join(".gqy/data"),
            cache_dir: root.join(".gqy/cache"),
            state_dir: root.join(".gqy/state"),
        },
    )
}
