/// Start receiving a message matching a given pattern.
///
/// This will spend 1 budget unit.
///
///
/// # Example
/// ```ignore
/// receive!({
///     match String: msg => {
///         println!("Received message: {}", msg);
///     },
///     after Duration::from_secs(1) => {
///         println!("Timeout occurred");
///     }
/// });
#[macro_export]
macro_rules! receive {
    ({
        $(
            match $ty:ty: $pat:pat_param  $( if $guard:expr )? => $block:block
        ),+ $(,)?
        $( after $timeout:expr => $timeout_block:block )? $(,)?
    }) => {{
        #[allow(unused_variables)]
        let timeout: Option<std::time::Duration> = None;
        $( let timeout = Some($timeout); )?

        let msg = $crate::global::recv_matching(timeout, |msg| {
                $(
                  if let Some(msg) = msg.downcast_ref::<$ty>() {
                      #[allow(unused_variables)]
                      #[allow(irrefutable_let_patterns)]
                      if let $pat = msg {
                          $(if !$guard { return false;} )?
                          return true;
                      }
                  }
                )*

                false
            })
            .await;

        match msg {
            Ok(msg) => {
                'inner: loop {
                $(
                    if let Some(msg_ref) = msg.downcast_ref::<$ty>() {
                        #[allow(unused_variables)]
                        #[allow(irrefutable_let_patterns)]
                        let is_match = if let $pat = msg_ref {
                            true
                        } else { false };

                        if is_match {
                            if let Some($pat) = msg.downcast::<$ty>().ok().map(|msg| *msg) {
                                $block
                            }

                            break 'inner;
                        }
                    }
                )*

                    unreachable!("recv_matching returned a message that did not match any arm");

                }
            },
            Err(_) => {
                $( $timeout_block )?
            },
        }

    }};
}

/// ```no_run
/// use std::time::Duration;
/// async fn test() {
///     kerosene::receive_new! {
///         match String {
///             s if s.is_empty() => println!("{}", s),
///         }
///         else {
///             println!("No match");
///         }
///         after Duration::from_secs(1) => println!("Timeout"),
///     }
/// }
/// ```
///
/// ```no_run
/// use std::time::Duration;
/// async fn test() {
///     kerosene::receive_new! {
///         match String {
///             s if s.is_empty() => println!("{}", s),
///         }
///         else msg {
///             // Do something with msg
///             println!("No match");
///         }
///         after Duration::from_secs(1) => println!("Timeout"),
///     }
/// }
/// ```
///
/// ```no_run
/// use std::time::Duration;
/// async fn test() {
///     kerosene::receive_new! {
///         match String {
///             s if s.is_empty() => println!("{}", s),
///         }
///         after Duration::from_secs(1) => println!("Timeout"),
///     }
/// }
/// ```
///
/// ```no_run
/// use std::time::Duration;
/// async fn test() {
///     kerosene::receive_new! {
///         match String {
///             s => println!("{}", s),
///         }
///         match i32 {
///             1 | 2 => println!("Match!"),
///             x => println!("No match"),
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! receive_new {
    {
        $(
            match $ty:ty {
                $($pat:pat $(if $guard:expr)? => $expr:expr),+ $(,)?
            }
        )*
        else $else:ident $else_block:block
    } => {{
        let msg = $crate::global::recv_matching(None, |_| true).await;

        match msg {
            Ok(msg) => {
                if false { unreachable!() }
                $(
                    else if msg.is::<$ty>() {
                        let msg = msg.downcast::<$ty>().unwrap();
                        match *msg {
                            $(
                                $pat $(if $guard)? => $expr,
                            )+
                            #[allow(unreachable_patterns)]
                            _ => unreachable!(),
                        }
                    }
                )*
                else {
                    let $else = msg;
                    $else_block
                }
            },
            Err(_) => {
                unreachable!()
            },
        }
    }};

    {
        $(
            match $ty:ty {
                $($pat:pat $(if $guard:expr)? => $expr:expr),+ $(,)?
            }
        )*
        else $else_block:block
    } => {{
        let msg = $crate::global::recv_matching(None, |_| true).await;

        match msg {
            Ok(msg) => {
                if false { unreachable!() }
                $(
                    else if msg.is::<$ty>() {
                        let msg = msg.downcast::<$ty>().unwrap();
                        match *msg {
                            $(
                                $pat $(if $guard)? => $expr,
                            )+
                            #[allow(unreachable_patterns)]
                            _ => unreachable!(),
                        }
                    }
                )*
                else {
                    $else_block
                }
            },
            Err(_) => {
                unreachable!()
            },
        }
    }};

    {
        $(
            match $ty:ty {
                $($pat:pat $(if $guard:expr)? => $expr:expr),+ $(,)?
            }
        )*
    } => {{
        let msg = $crate::global::recv_matching(None, |msg| {
            $(
                if let Some(msg) = msg.downcast_ref::<$ty>() {
                    match msg {
                        $(
                            #[allow(unused_variables)]
                            $pat $(if $guard)? => return true,
                        )+
                        #[allow(unreachable_patterns)]
                        _ => (),
                    }
                }

            )*

            false
        }).await;

        match msg {
            Ok(msg) => {
                if false { unreachable!() }
                $(
                    else if msg.is::<$ty>() {
                        let msg = msg.downcast::<$ty>().unwrap();
                        match *msg {
                            $(
                                $pat $(if $guard)? => $expr,
                            )+
                            #[allow(unreachable_patterns)]
                            _ => unreachable!(),
                        }
                    }
                )*
                else { unreachable!() }
            },
            Err(_) => {
                unreachable!()
            },
        }
    }};

    {
        $(
            match $ty:ty {
                $($pat:pat $(if $guard:expr)? => $expr:expr),+ $(,)?
            }
        )*
        else $else:ident $else_block:block
        after $timeout:expr => $timeout_block:expr $(,)?
    } => {{
        let msg = $crate::global::recv_matching(Some($timeout), |_| true).await;

        match msg {
            Ok(msg) => {
                if false { unreachable!() }
                $(
                    else if msg.is::<$ty>() {
                        let msg = msg.downcast::<$ty>().unwrap();
                        match *msg {
                            $(
                                $pat $(if $guard)? => $expr,
                            )+
                            #[allow(unreachable_patterns)]
                            _ => unreachable!(),
                        }
                    }
                )*
                else {
                    let $else = msg;
                    $else_block
                }
            },
            Err(_) => {
                $timeout_block
            },
        }
    }};

    {
        $(
            match $ty:ty {
                $($pat:pat $(if $guard:expr)? => $expr:expr),+ $(,)?
            }
        )*
        else $else_block:block
        after $timeout:expr => $timeout_block:expr $(,)?
    } => {{
        let msg = $crate::global::recv_matching(Some($timeout), |_| true).await;

        match msg {
            Ok(msg) => {
                if false { unreachable!() }
                $(
                    else if msg.is::<$ty>() {
                        let msg = msg.downcast::<$ty>().unwrap();
                        match *msg {
                            $(
                                $pat $(if $guard)? => $expr,
                            )+
                            #[allow(unreachable_patterns)]
                            _ => unreachable!(),
                        }
                    }
                )*
                else {
                    $else_block
                }
            },
            Err(_) => {
                $timeout_block
            },
        }
    }};

    {
        $(
            match $ty:ty {
                $($pat:pat $(if $guard:expr)? => $expr:expr),+ $(,)?
            }
        )*
        after $timeout:expr => $timeout_block:expr $(,)?
    } => {{
        let msg = $crate::global::recv_matching(Some($timeout), |msg| {
            $(
                if let Some(msg) = msg.downcast_ref::<$ty>() {
                    match msg {
                        $(
                            #[allow(unused_variables)]
                            $pat $(if $guard)? => return true,
                        )+
                        #[allow(unreachable_patterns)]
                        _ => (),
                    }
                }

            )*

            false
        }).await;

        match msg {
            Ok(msg) => {
                if false { unreachable!() }
                $(
                    else if msg.is::<$ty>() {
                        let msg = msg.downcast::<$ty>().unwrap();
                        match *msg {
                            $(
                                $pat $(if $guard)? => $expr,
                            )+
                            #[allow(unreachable_patterns)]
                            _ => unreachable!(),
                        }
                    }
                )*
                else { unreachable!() }
            },
            Err(_) => {
                $timeout_block
            },
        }
    }};
}
