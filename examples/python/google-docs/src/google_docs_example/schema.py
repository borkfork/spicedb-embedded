"""SpiceDB schema for a Google Docs–style drive."""

DRIVE_SCHEMA = """
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
""".strip()
