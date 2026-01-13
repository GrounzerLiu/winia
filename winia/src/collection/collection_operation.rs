#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionOperation {
    Add(usize),
    Remove(usize),
    Update(usize),
    UpdateAll,
    Clear,
}