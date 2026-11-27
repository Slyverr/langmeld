/// One part after another, as [`pattern::seq`](crate::pattern::seq).
#[macro_export]
macro_rules! seq {
    ($($part:expr),+ $(,)?) => {
        $crate::pattern::seq([$($part),+])
    };
}

/// One of several parts, as [`pattern::choice`](crate::pattern::choice).
#[macro_export]
macro_rules! choice {
    ($($alternative:expr),+ $(,)?) => {
        $crate::pattern::choice([$($alternative),+])
    };
}
