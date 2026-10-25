// Compile this source using default, aliased, and facade-only dependencies.
// CASE! becomes either Rust's formatter or independent prototype rendering.
struct DisplayOnly<'a>(&'a str);
impl std::fmt::Display for DisplayOnly<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
#[derive(Debug)]
struct Dual<'a>(&'a str);
impl std::fmt::Display for Dual<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
impl Dual<'_> {
    fn show(&self) { CASE!("{self} {self:?}"); }
}
fn main() {
    CASE!("answer={}", 42);
    let city = String::from("Rochester");
    let café = 7;
    CASE!("{city} {café} {city:?}");
    let r#async = DisplayOnly("keyword capture");
    CASE!("{async}");
    CASE!("{type}", type = DisplayOnly("keyword label"));
    let mut value = Dual("borrowed");
    let mut calls = 0;
    CASE!("{0} {0:?} {0}", { calls += 1; &mut value });
    assert_eq!(calls, 1);
    value.0 = "after";
    value.show();
    assert_eq!(city, "Rochester");

    // Forward token trees to preserve the literal's caller-side syntax context.
    macro_rules! caller_wrapper { ($($input:tt)*) => { CASE!($($input)*) }; }
    let wrapped = DisplayOnly("wrapped caller");
    caller_wrapper!("{wrapped}");
}
