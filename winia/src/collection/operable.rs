use crate::collection::CollectionOperation;

pub trait OperableClone {
    fn clone_box(&self) -> Box<dyn Operable>;
}

pub trait Operable : OperableClone + Send + Sync + 'static {
    fn take_operations(&mut self) -> Vec<CollectionOperation>;
}