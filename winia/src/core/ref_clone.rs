pub trait RefClone: Sized {
    /// Returns a new reference to `self`.
    /// In general, all properties of the type that implements this trait should be of type Arc or Rc.
    fn ref_clone(&self) -> Self;
}