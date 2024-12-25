use crate::core::get_id_by_str;

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
         use $crate:core::get_id_by_str;
         Target::Exclusion(vec![$(get_id_by_str($target)),+])
     }
}

#[macro_export]
macro_rules! include_target {
    () => {
        Target::Inclusion(vec![])
    };
     ($($target:expr),+ $(,)?) => {
         {
            use $crate::core::get_id_by_str;
            Target::Inclusion(vec![$(get_id_by_str($target).unwrap()),+])
        }
     }
}
