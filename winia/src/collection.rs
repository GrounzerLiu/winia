pub mod w_vec;
mod w_hash_map;
mod w_linked_list;

pub use w_vec::WVec;
pub use w_hash_map::WHashMap;
pub use w_linked_list::WLinkedList;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Add(usize),
    Remove(usize),
    Update(usize),
    Clear,
    Other,
}