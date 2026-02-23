//! SpiceDB schema for a Google Docs–style drive: folders (with optional parent)
//! and documents that live in a folder. Permissions flow folder → document and
//! parent folder → child folder.

pub const DRIVE_SCHEMA: &str = r#"
definition user {}

definition folder {
  relation parent: folder
  relation viewer: user
  relation editor: user

  permission view = viewer + parent->view
  permission edit = editor + parent->edit
}

definition document {
  relation folder: folder
  relation viewer: user
  relation editor: user

  permission view = viewer + folder->view
  permission edit = editor + folder->edit
}
"#;
