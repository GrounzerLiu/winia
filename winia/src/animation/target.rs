use crate::ui::item::Selector;

#[derive(Debug, Clone)]
pub enum Target {
    Exclusion(Vec<Selector>),
    Inclusion(Vec<Selector>),
}

#[macro_export]
macro_rules! exclude_target {
    () => {
        $crate::animation::Target::Exclusion(vec![])
    };
    ($($target:expr),+ $(,)?) => {
        {
            $crate::animation::Target::Exclusion(
                vec![
                    $(
                    $crate::animation::Selector::from($target),
                    )*
                ]
            )
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
            $crate::animation::Target::Inclusion(
                vec![
                    $(
                    $crate::animation::Selector::from($target),
                    )*
                ]
            )
       }
    }
}