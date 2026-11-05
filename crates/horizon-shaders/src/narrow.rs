//! What of a program a GPU driver refuses, narrowed down to a function: the program is tried
//! again with one function's body left empty (it returns zero at once), first each stage's
//! `main`, then down the calls from the one that helped.

use naga::{Block, Expression, Function, Handle, Module, Span, Statement};

/// Which of the program's stages, and down the calls from its `main` which function, the GPU
/// driver refuses: `compiles(vertex, fragment)` tries a pair of modules, twice (each stage's
/// `main` emptied) and then while `go_on(tries so far)`. Returns a note for the log.
pub fn narrow_down(
    vertex: &Module,
    fragment: &Module,
    mut go_on: impl FnMut(usize) -> bool,
    mut compiles: impl FnMut(Module, Module) -> bool,
) -> String {
    let (Some(vertex_main), Some(fragment_main)) = (user_main(vertex), user_main(fragment)) else {
        return "a stage has no main to empty".into();
    };
    let mut tried = 1;
    let (stage, module, main) = if compiles(vertex.clone(), emptied(fragment, fragment_main)) {
        ("fragment", fragment, fragment_main)
    } else {
        tried += 1;
        if compiles(emptied(vertex, vertex_main), fragment.clone()) {
            ("vertex", vertex, vertex_main)
        } else {
            return "it does not compile even with either stage's main emptied (2 tries)".into();
        }
    };
    let mut path = vec![name(module, main)];
    let mut at = main;
    let (mut cut, mut callees) = (false, Vec::new());
    'down: loop {
        callees.clear();
        calls(&module.functions[at].body, &mut callees);
        // The most code first.
        callees.sort_by_cached_key(|&f| std::cmp::Reverse(weight(module, f)));
        for &f in &callees {
            if !go_on(tried) {
                cut = true;
                break 'down;
            }
            tried += 1;
            let without = emptied(module, f);
            let ok = match stage {
                "fragment" => compiles(vertex.clone(), without),
                _ => compiles(without, fragment.clone()),
            };
            if ok {
                path.push(name(module, f));
                at = f;
                continue 'down;
            }
        }
        break;
    }
    let last = name(module, at);
    let path = path.join(" > ");
    match callees.len() {
        _ if cut => format!(
            "it compiles with the {stage} shader's {last} emptied ({path}; stopped after {tried} \
             tries)"
        ),
        0 => format!(
            "it compiles with the {stage} shader's {last} emptied, which calls no other function \
             ({path}; {tried} tries)"
        ),
        n => format!(
            "it compiles with the {stage} shader's {last} emptied, but not with {} it calls \
             emptied instead ({path}; {tried} tries)",
            if n == 1 {
                "the one function".to_string()
            } else {
                format!("any one of the {n} functions")
            }
        ),
    }
}

/// `module` with function `f` returning at once (a zero value), and what only it used removed.
pub fn emptied(module: &Module, f: Handle<Function>) -> Module {
    let mut module = module.clone();
    let function = &mut module.functions[f];
    let value = function.result.as_ref().map(|result| {
        function
            .expressions
            .append(Expression::ZeroValue(result.ty), Span::UNDEFINED)
    });
    function.body = Block::from_vec(vec![Statement::Return { value }]);
    function.named_expressions.clear();
    naga::compact::compact(&mut module, naga::compact::KeepUnused::No);
    module
}

/// The function the entry point calls: the program's `main` (ours, which calls the pack's).
fn user_main(module: &Module) -> Option<Handle<Function>> {
    let entry = module.entry_points.first()?;
    entry.function.body.iter().find_map(|s| match *s {
        Statement::Call { function, .. } => Some(function),
        _ => None,
    })
}

/// The functions `block` calls, each once, in the order of their first call.
fn calls(block: &Block, out: &mut Vec<Handle<Function>>) {
    for s in block.iter() {
        match s {
            Statement::Call { function, .. } if !out.contains(function) => out.push(*function),
            Statement::Block(b) => calls(b, out),
            Statement::If { accept, reject, .. } => {
                calls(accept, out);
                calls(reject, out);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    calls(&case.body, out);
                }
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                calls(body, out);
                calls(continuing, out);
            }
            _ => {}
        }
    }
}

/// How much code running `f` takes: its expressions and those of everything it calls, each
/// function counted once.
fn weight(module: &Module, f: Handle<Function>) -> usize {
    let mut seen = vec![f];
    let mut i = 0;
    while i < seen.len() {
        let mut more = Vec::new();
        calls(&module.functions[seen[i]].body, &mut more);
        for m in more {
            if !seen.contains(&m) {
                seen.push(m);
            }
        }
        i += 1;
    }
    seen.iter()
        .map(|&h| module.functions[h].expressions.len())
        .sum()
}

fn name(module: &Module, f: Handle<Function>) -> String {
    module.functions[f]
        .name
        .clone()
        .unwrap_or_else(|| format!("function {}", f.index()))
}

#[cfg(test)]
mod tests {
    use naga::ShaderStage;
    use naga::front::glsl::{Frontend, Options};
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    use super::*;

    fn module(stage: ShaderStage, source: &str) -> Module {
        Frontend::default()
            .parse(&Options::from(stage), source)
            .unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn valid(module: &Module) -> bool {
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(module)
            .is_ok()
    }

    /// Whether `module` still has function `name` with a body of its own.
    fn has(module: &Module, name: &str) -> bool {
        module
            .functions
            .iter()
            .any(|(_, f)| f.name.as_deref() == Some(name) && f.body.len() > 1)
    }

    const VERTEX: &str = "#version 450\nlayout(location = 0) out vec2 uv;\n\
        vec2 corner(int i) { return vec2(float(i & 1), float(i >> 1)); }\n\
        void main() { uv = corner(int(gl_VertexIndex)); gl_Position = vec4(uv, 0.0, 1.0); }\n";
    const FRAGMENT: &str = "#version 450\nlayout(location = 0) in vec2 uv;\n\
        layout(location = 0) out vec4 color;\n\
        float trail(vec2 p) {\n\
            float d = distance(p.x, 0.5); if (d > 0.4) { return 0.0; } return d; }\n\
        float stars(vec2 p) {\n\
            float s = 0.0; for (int i = 0; i < 3; i++) { s += trail(p * float(i)); } return s; }\n\
        vec3 sky(vec2 p) { return vec3(p, 1.0) * 0.5; }\n\
        void main() { vec3 c = sky(uv); c += stars(uv); color = vec4(c, 1.0); }\n";

    #[test]
    fn emptied_modules_validate() {
        for (stage, source) in [
            (ShaderStage::Vertex, VERTEX),
            (ShaderStage::Fragment, FRAGMENT),
        ] {
            let m = module(stage, source);
            for (f, _) in m.functions.iter() {
                let e = emptied(&m, f);
                assert!(valid(&e), "{:?} emptied", m.functions[f].name);
                assert_eq!(e.entry_points.len(), 1);
            }
        }
    }

    #[test]
    fn narrows_down_to_the_function_the_driver_refuses() {
        let (v, f) = (
            module(ShaderStage::Vertex, VERTEX),
            module(ShaderStage::Fragment, FRAGMENT),
        );
        let any = |tried| tried < 24;
        // A driver that refuses `trail`'s own code.
        let mut tries = 0;
        let note = narrow_down(&v, &f, any, |vertex, fragment| {
            tries += 1;
            assert!(valid(&vertex) && valid(&fragment));
            !has(&fragment, "trail")
        });
        assert_eq!(
            note,
            format!(
                "it compiles with the fragment shader's trail emptied, which calls no other \
                 function (main > stars > trail; {tries} tries)"
            )
        );
        // One that refuses `stars` whatever it calls.
        let note = narrow_down(&v, &f, any, |_, fragment| !has(&fragment, "stars"));
        assert!(
            note.starts_with(
                "it compiles with the fragment shader's stars emptied, but not with the one \
                 function it calls emptied instead (main > stars; "
            ),
            "{note}"
        );
        // One that refuses the vertex shader's `corner`.
        let note = narrow_down(&v, &f, any, |vertex, _| !has(&vertex, "corner"));
        assert!(note.contains("vertex shader's corner emptied"), "{note}");
        // One that refuses everything.
        let note = narrow_down(&v, &f, any, |_, _| false);
        assert_eq!(
            note,
            "it does not compile even with either stage's main emptied (2 tries)"
        );
        // A short budget stops on the way down.
        let note = narrow_down(
            &v,
            &f,
            |tried| tried < 2,
            |_, fragment| !has(&fragment, "trail"),
        );
        assert_eq!(
            note,
            "it compiles with the fragment shader's stars emptied (main > stars; stopped after 2 \
             tries)"
        );
    }
}
