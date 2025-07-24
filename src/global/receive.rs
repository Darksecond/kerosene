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

// ```
// receive! {
//     match String {
//         s => println!(s),
//     }
//     else {
//         println!("No match");
//     }
//     after Duration::from_secs(1) => println!("Timeout"),
// }
// ```
