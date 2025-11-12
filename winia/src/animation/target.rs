#[derive(Debug, Clone)]
pub enum Target {
    Exclusion(Vec<u32>),
    Inclusion(Vec<u32>),
}

#[macro_export]
macro_rules! exclude_target {
    () => {
        $crate::animation::Target::Exclusion(vec![])
    };
    ($($target:expr),+ $(,)?) => {
        {
            use $crate::core::get_id_by_name;
            $crate::animation::Target::Exclusion(vec![$(get_id_by_name($target).unwrap()),+])
        }
    }
}

#[macro_export]
macro_rules! include_target {
    () => {
        $crate::ui::animation::Target::Inclusion(vec![])
    };
    ($($target:expr),+ $(,)?) => {
        {
           use $crate::core::get_id_by_name;
           $crate::ui::animation::Target::Inclusion(vec![$(get_id_by_name($target).unwrap()),+])
       }
    }
}