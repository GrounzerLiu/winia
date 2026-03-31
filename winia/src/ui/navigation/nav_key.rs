use std::any::Any;

pub trait NavKey: Any + Send + Sync {
    fn nav_key(&self) -> &'static str;

    fn instance_key(&self) -> String {
        self.nav_key().to_string()
    }
}
