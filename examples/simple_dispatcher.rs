// In this example, we crate a simple dispatcher with 3 possible event:
// 1. `ping`. This command simply returns "pong" answer.
// 2. `print_value`. This command prints the value stored in the program.
// 3. `set_value`. This command set the value that is stored in the program.
//
// Usage:
// ```
// >> ping
// Pong
// >> print_value
// 0
// >> set_value 123
// 123 stored
// >> print_value
// 123
// ```

extern crate dispatch_tree as dptree;

use dispatch_tree::parser::Parseable;
use dispatch_tree::Handler;
use std::io::Write;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

// First, we declare the type for incoming events.
#[derive(Debug)]
enum Event {
    Ping,
    SetValue(SetValueEvent),
    PrintValue,
}

// We also need to create a newtype for `set_value` event. Further it will be explained why.
#[derive(Debug)]
struct SetValueEvent(i32);

// User will input text from the console - so we declare method to parse user input to our type.
impl Event {
    fn parse(input: &[&str]) -> Option<Self> {
        match input {
            ["ping"] => Some(Event::Ping),
            ["set_value", value] => Some(Event::SetValue(SetValueEvent(value.parse().unwrap()))),
            ["print"] => Some(Event::PrintValue),
            _ => None,
        }
    }
}

// That is why we have declared newtype for `set_value` command. We want us to be able to access
// the value to which the user wants to set the stored value in the program.
// Implementing `Parseable` allow to us to parse from `Event` to `SetValueEvent`. This is the
// same as when we parse input string "123" to number 123: just concretization of the
// content of the event.
impl Parseable<SetValueEvent> for Event {
    type Rest = ();

    // Parsing `Event` -> `SetValueEvent`.
    fn parse(self) -> Result<(SetValueEvent, Self::Rest), Self> {
        match self {
            Event::SetValue(e) => Ok((e, ())),
            _ => Err(self),
        }
    }

    // Recombining `SetValueEvent` -> `Event`. To learn more about this function,
    // see documentation of `Parseable` trait.
    fn recombine(data: (SetValueEvent, Self::Rest)) -> Self {
        Event::SetValue(data.0)
    }
}

// Second, we declare handlers constructors.
// This function will construct a handler that handle `ping` event.
#[rustfmt::skip]
fn ping_handler() -> impl Handler<Event, Res = String> {
    // Let's take a closer look.
    // We create here 2 handlers which are chained. This is called the `Chain Responsibility Pattern`.
    // First handler is `dptree::filter` that constructs `Filter` handler. It allows
    // filtering input event by some condition `Fn(&Event) -> bool`. In that case we want
    // pass only `ping` events, so we use `dptree::matches!` macro that is lazy variant of
    // `std::matches!` macro.
    dptree::filter(dptree::matches!(Event::Ping))
        // After a filter, we give only events that satisfies the condition. In our case it is
        // only `ping` events. We must handle that event - so we use `EndPoint` handler that allow
        // to handle all incoming events. In the handler we just returns `"Pong"` string, because we
        // know that earlier `Filter` accepts only `ping` event.
        .end_point(|| async { "Pong".to_string() })
}

// This function will construct a handler that handle `set_value` event.
#[rustfmt::skip]
fn set_value_handler(store: Arc<AtomicI32>) -> impl Handler<Event, Res = String> {
    // In this case in the endpoint we _must_ know to which value user want set program value. So
    // in this case we cannot use `Filter` as above because it does not provide information of the
    // internal representation of the event. So we use another handler - `Parser`. `Parser` allow
    // us to parse one event type to another. In our case we want to parse `Event` to `SetValueEvent`.
    // If input is `Event::SetValueEvent`, it will be parsed to the `SetValueEvent` newtype and passed
    // to the next handler.
    dptree::parser::<Event, SetValueEvent>()
        // Next, handle the `set_value` event.
        .end_point(
            move |SetValueEvent(value): SetValueEvent| {
                // Clone store to use in `async` block.
                let store = store.clone();
                async move {
                    // Store user input to store.
                    store.store(value, Ordering::SeqCst);
                    // Return info that value are stored.
                    format!("{} stored", value)
                }
            },
        )
}

// This function will construct a handler that handle `print_value` event.
#[rustfmt::skip]
fn print_value_handler(store: Arc<AtomicI32>) -> impl Handler<Event, Res = String> {
    // Filter only `Event::PrintValue` events.
    dptree::filter(dptree::matches!(Event::PrintValue))
        .end_point(move || {
            let store = store.clone();
            async move {
                let value = store.load(Ordering::SeqCst);
                // Return value.
                format!("{}", value)
            }
        })
}

#[tokio::main]
async fn main() {
    // Create program store.
    let store = Arc::new(AtomicI32::new(0));

    // When we write all of our constructors - there are a question: how can we combine them?
    // For that purpose we use `Node` handler. It does a simple job: passed input event to all
    // handlers that it have and wait until the event is processed. If no one endpoint process
    // the event, `Node` will return an error.
    let dispatcher = dptree::node::<Event, String>()
        // Add all our handlers.
        .and(ping_handler())
        .and(set_value_handler(store.clone()))
        .and(print_value_handler(store.clone()))
        .build();

    // Simple REPL for the constructed dispatcher.
    loop {
        print!(">> ");
        std::io::stdout().flush().unwrap();

        let mut cmd = String::new();
        std::io::stdin().read_line(&mut cmd).unwrap();

        let strs = cmd.trim().split(" ").collect::<Vec<_>>();
        let event = Event::parse(strs.as_slice());

        let out = match event {
            Some(event) => dispatcher.handle(event).await.unwrap(),
            _ => "Unknown command".to_string(),
        };
        println!("{}", out);
    }
}
