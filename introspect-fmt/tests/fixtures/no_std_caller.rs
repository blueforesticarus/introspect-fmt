// Alias std only for the executable's output; ::std must remain unavailable
// to macro expansions, as in no_std dependencies logging into a std app.
#![no_std]
extern crate std as host;
use host::println;

fn main() {
    let number = 31u32;
    CASE!("value {number:#06x}");
    CASE!("temporary {}", host::string::String::from("borrowed").as_str());
}
