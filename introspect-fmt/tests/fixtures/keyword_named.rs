// CASE! is compiled as Rust's formatter and the prototype, then outputs are
// compared. Keep keyword coverage here so a parser rejection cannot prevent
// unrelated integration tests from running.
fn main() {
    CASE!("{as} {async} {await} {break} {const} {continue} {crate} {dyn}", as = 0, async = 1, await = 2, break = 3, const = 4, continue = 5, crate = 6, dyn = 7);
    CASE!("{else} {enum} {extern} {false} {fn} {for} {if} {impl}", else = 0, enum = 1, extern = 2, false = 3, fn = 4, for = 5, if = 6, impl = 7);
    CASE!("{in} {let} {loop} {match} {mod} {move} {mut} {pub}", in = 0, let = 1, loop = 2, match = 3, mod = 4, move = 5, mut = 6, pub = 7);
    CASE!("{ref} {return} {self} {Self} {static} {struct} {super} {trait}", ref = 0, return = 1, self = 2, Self = 3, static = 4, struct = 5, super = 6, trait = 7);
    CASE!("{true} {type} {unsafe} {use} {where} {while} {abstract} {become}", true = 0, type = 1, unsafe = 2, use = 3, where = 4, while = 5, abstract = 6, become = 7);
    CASE!("{box} {do} {final} {gen} {macro} {override} {priv} {try}", box = 0, do = 1, final = 2, gen = 3, macro = 4, override = 5, priv = 6, try = 7);
    CASE!("{typeof} {unsized} {virtual} {yield} {auto} {default} {macro_rules} {raw}", typeof = 0, unsized = 1, virtual = 2, yield = 3, auto = 4, default = 5, macro_rules = 6, raw = 7);
    CASE!("{safe} {union}", safe = 0, union = 1);

    // Keywords in positional expressions must not be mistaken for labels.
    CASE!("{} {:?}", true == false, if true { Some(1) } else { None });
    CASE!("{match}", match = match 1 { 1 => "matched", _ => "other" });
    let left = 1;
    let right = 2;
    CASE!("{} {}", left == right, left != right);

    // Named keyword arguments must retain only the requested formatting hooks.
    struct DisplayOnly<'a>(&'a str);
    impl std::fmt::Display for DisplayOnly<'_> {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.0)
        }
    }
    #[derive(Debug)]
    struct DebugOnly;
    let mut calls = 0;
    let mut value = DisplayOnly("borrowed");
    CASE!("{type} {self:?} {type}", type = { calls += 1; &mut value }, self = DebugOnly);
    assert_eq!(calls, 1);
    assert_eq!(value.0, "borrowed");

    // Raw identifiers refer to the same format argument name as bare keywords.
    CASE!("{type}", r#type = DisplayOnly("explicit raw"));
    let r#async = DisplayOnly("captured raw");
    CASE!("{async} {async}");
}
