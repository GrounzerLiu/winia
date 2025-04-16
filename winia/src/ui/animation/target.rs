#[derive(Debug, Clone)]
pub enum Target {
    Exclusion(Vec<usize>),
    Inclusion(Vec<usize>),
}

#[macro_export]
macro_rules! exclude_target {
    () => {
        Target::Exclusion(vec![])
    };
    ($($target:expr),+ $(,)?) => {
        {
            use $crate::core::get_id_by_name;
            $crate::ui::animation::Target::Exclusion(vec![$(get_id_by_name($target).unwrap()),+])
        }
    }
}

#[macro_export]
macro_rules! include_target {
    () => {
        Target::Inclusion(vec![])
    };
    ($($target:expr),+ $(,)?) => {
        {
           use $crate::core::get_id_by_name;
           $crate::ui::animation::Target::Inclusion(vec![$(get_id_by_name($target).unwrap()),+])
       }
    }
}
