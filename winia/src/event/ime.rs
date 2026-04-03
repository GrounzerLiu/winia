#[derive(Clone, Debug)]
pub enum Ime {
    Enabled,
    Enter,
    Delete,
    PreEdit(String, Option<(usize, usize)>),
    Commit(String),
    Disabled,
    DeleteSurrounding {
        /// Bytes to remove before the selection
        before_bytes: usize,
        /// Bytes to remove after the selection
        after_bytes: usize,
    },
}