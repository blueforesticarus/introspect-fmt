// Capturing self is common inside Display methods and ordinary object methods.
struct Label;
impl std::fmt::Display for Label {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("label")
    }
}
impl Label {
    fn log(&self) {
        CASE!("{self}");
    }
}
fn main() {
    Label.log();
}
