//! Reads real packs as black boxes: `HORIZON_TEST_SHADER_PACKS=<folder> cargo test -p
//! horizon-shaders --release -- --ignored --nocapture`. The folder (and its subfolders) holds
//! pack zips or folders, which are never committed; this prints what was read and how long it took.

use std::path::Path;
use std::time::Instant;

use std::collections::BTreeMap;

use horizon_shaders::{
    Lang, Macros, Pack, Properties, Target, dimension_folder, discover, load_post, preprocess,
    translate_composite,
};

#[test]
#[ignore = "needs HORIZON_TEST_SHADER_PACKS"]
fn real_packs_read() {
    let dir = std::env::var_os("HORIZON_TEST_SHADER_PACKS").expect("HORIZON_TEST_SHADER_PACKS");
    let dir = Path::new(&dir);
    let mut found = discover(dir);
    for sub in std::fs::read_dir(dir).unwrap().flatten() {
        if sub.path().is_dir() {
            found.extend(discover(&sub.path()));
        }
    }
    let mut read = 0;
    for f in found.iter().filter(|f| f.problem.is_none()) {
        let start = Instant::now();
        let pack = Pack::open(&f.path).unwrap();
        let macros = Macros::standard();
        let shaders = Properties::parse(
            &pack.read_text("shaders.properties").unwrap_or_default(),
            &horizon_shaders::with_options(&pack, &macros),
        );
        let ids: Vec<Properties> = ["block", "item", "entity", "dimension"]
            .iter()
            .filter_map(|n| pack.read_text(&format!("{n}.properties")))
            .map(|t| Properties::parse(&t, &macros))
            .collect();
        let lang = Lang::load(&pack, "en_us");
        let elapsed = start.elapsed();
        println!(
            "{}: shaders.properties {} keys ({} notes), {} id files {} keys ({} notes), lang {}, {:.2} ms",
            f.name,
            shaders.entries.len(),
            shaders.notes.len(),
            ids.len(),
            ids.iter().map(|p| p.entries.len()).sum::<usize>(),
            ids.iter().map(|p| p.notes.len()).sum::<usize>(),
            if lang == Lang::default() {
                "none"
            } else {
                "read"
            },
            elapsed.as_secs_f64() * 1000.0
        );
        for n in &shaders.notes {
            println!("  shaders.properties:{}: {}", n.line, n.message);
        }
        // Every stage file of the base and dimension folders through the preprocessor.
        let start = Instant::now();
        let (mut stages, mut lines, mut diagnostics) = (0, 0, Vec::new());
        for dir in ["", "world0", "world-1", "world1"] {
            for file in pack.list(dir) {
                if [".vsh", ".fsh", ".gsh", ".csh"]
                    .iter()
                    .any(|e| file.ends_with(e))
                {
                    let path = if dir.is_empty() {
                        file
                    } else {
                        format!("{dir}/{file}")
                    };
                    let p = preprocess(&pack, &path, &macros);
                    stages += 1;
                    lines += p.lines.len();
                    diagnostics.extend(p.diagnostics.iter().map(|d| {
                        let file = &p.files[d.origin.file as usize];
                        format!("{file}:{}: {}", d.origin.line, d.message)
                    }));
                }
            }
        }
        println!(
            "  {stages} stage files preprocessed, {lines} lines out, {} diagnostics, {:.1} ms",
            diagnostics.len(),
            start.elapsed().as_secs_f64() * 1000.0
        );
        diagnostics.sort();
        diagnostics.dedup();
        for d in diagnostics.iter().take(8) {
            println!("    {d}");
        }
        // Every composite-style program with both stages, translated and validated.
        let start = Instant::now();
        let (mut ok, mut failed, mut causes) = (0, 0, BTreeMap::<String, usize>::new());
        for dir in ["", "world0", "world-1", "world1"] {
            for file in pack.list(dir) {
                let Some(name) = file.strip_suffix(".fsh") else {
                    continue;
                };
                let composite = [
                    "composite",
                    "deferred",
                    "final",
                    "prepare",
                    "begin",
                    "shadowcomp",
                ]
                .iter()
                .any(|p| {
                    name.strip_prefix(p)
                        .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
                });
                let path = |ext: &str| {
                    if dir.is_empty() {
                        format!("{name}.{ext}")
                    } else {
                        format!("{dir}/{name}.{ext}")
                    }
                };
                if !composite || !pack.exists(&path("vsh")) {
                    continue;
                }
                let target = if name == "final" {
                    Target::Window
                } else {
                    Target::Buffers
                };
                let v = preprocess(&pack, &path("vsh"), &macros);
                let fr = preprocess(&pack, &path("fsh"), &macros);
                match translate_composite(&v, &fr, target) {
                    Ok(_) => ok += 1,
                    Err(errors) => {
                        failed += 1;
                        let first = &errors[0];
                        let cause: String = first.message.chars().take(90).collect();
                        *causes.entry(cause).or_default() += 1;
                        if std::env::var_os("SHOW_ERRORS").is_some() {
                            println!(
                                "    {}: {}:{}: {}",
                                path("fsh"),
                                first.file,
                                first.line,
                                first.message
                            );
                        }
                    }
                }
            }
        }
        println!(
            "  composite-style programs: {ok} translate, {failed} fail, {:.0} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        let mut causes: Vec<_> = causes.into_iter().collect();
        causes.sort_by_key(|c| std::cmp::Reverse(c.1));
        for (cause, n) in causes.iter().take(6) {
            println!("    {n} x {cause}");
        }
        // The post-processing set of each dimension, as the game loads it.
        for id in [0, -1, 1] {
            let start = std::time::Instant::now();
            let folder = dimension_folder(&pack, id);
            // Vertex inputs like the game's (gfx/pack_post.rs writes the real ones).
            let terrain = horizon_shaders::GbuffersInputs {
                declarations: "layout(location = 0) in vec3 hz_v_pos;".into(),
                body: "    hz_Vertex = vec4(hz_v_pos, 1.0);\n    hz_Color = vec4(1.0);\n".into(),
                attributes: vec![("mc_Entity".into(), "vec4(-1.0)".into())],
                ..Default::default()
            };
            let inputs = horizon_shaders::GameInputs {
                terrain: Some(terrain.clone()),
                sky: Some(terrain.clone()),
                celestial: Some(terrain.clone()),
                entities: Some(terrain.clone()),
                clouds: Some(terrain.clone()),
                weather: Some(terrain.clone()),
                hand: Some(terrain.clone()),
                terrain_geometry: Some(horizon_shaders::GeometryInputs {
                    inputs: horizon_shaders::GbuffersInputs {
                        declarations: format!(
                            "layout(set = 3, binding = 1) readonly buffer HzVertices {{ uint hz_vertices[]; }};\n{}",
                            terrain.declarations
                        ),
                        ..terrain
                    },
                    pull: "        hz_v_pos = uintBitsToFloat(uvec3(hz_vertices[hz_vertex_index * 6u], \
                           hz_vertices[hz_vertex_index * 6u + 1u], hz_vertices[hz_vertex_index * 6u + 2u]));\n"
                        .into(),
                    index: "uint(@)".into(),
                }),
            };
            let mut macros = Macros::standard();
            if std::env::var_os("SUBGROUPS").is_some() {
                macros.define("MC_GL_KHR_shader_subgroup_arithmetic", "");
            }
            for note in macros.define_features(&shaders) {
                println!("    {note}");
            }
            let post = load_post(&pack, &folder, &shaders, &macros, &inputs);
            if std::env::var_os("BACKENDS").is_some() {
                backends(&post, &id.to_string());
            }
            for p in post.programs.iter().chain(&post.shadow_comp) {
                if std::env::var("PRINT_PROGRAM").is_ok_and(|n| n == p.name)
                    && let Some(program) = &p.program
                {
                    for s in &program.interface.samplers {
                        println!(
                            "    {} sampler {} {} at {}",
                            p.name, s.ty, s.name, s.binding
                        );
                    }
                    for i in &program.interface.images {
                        println!(
                            "    {} image {} {} {} at {}",
                            p.name, i.ty, i.name, i.format, i.binding
                        );
                    }
                }
                for (name, c) in &p.compute {
                    println!(
                        "    {name} before {}: {} images, groups {:?} / {:?}, local {:?}",
                        p.name,
                        c.interface.images.len(),
                        c.work_groups,
                        c.work_groups_render,
                        c.module.entry_points[0].workgroup_size
                    );
                }
            }
            println!(
                "  dimension {id} (folder `{folder}`): {} post programs, shadow pass {} ({:?}, {:?}, {:?}, {:?}, {} shadowcomp), terrain {:?}, water {:?}, sky {:?} {:?}, entities {:?} {:?}, {} notes, {:.0} ms",
                post.programs.len(),
                post.shadow_pass,
                post.shadow.as_ref().map(|t| &t.name),
                post.shadow_water.as_ref().map(|t| &t.name),
                post.shadow_entities.as_ref().map(|t| &t.name),
                post.shadow_block.as_ref().map(|t| &t.name),
                post.shadow_comp.len(),
                post.terrain.as_ref().map(|t| &t.name),
                post.water.as_ref().map(|t| &t.name),
                post.sky.as_ref().map(|t| &t.name),
                post.sky_textured.as_ref().map(|t| &t.name),
                post.entities.as_ref().map(|t| &t.name),
                post.block.as_ref().map(|t| &t.name),
                post.notes.len() + post.config.notes.len(),
                start.elapsed().as_secs_f64() * 1000.0
            );
            let mut custom = post.custom;
            custom.eval(0.016, &|_| None);
            println!(
                "    {} custom uniforms; notes: {:?}",
                custom.uniforms().count(),
                custom.notes.iter().take(4).collect::<Vec<_>>()
            );
            let shown = if std::env::var_os("SHOW_ERRORS").is_some() {
                usize::MAX
            } else {
                12
            };
            for n in post.notes.iter().chain(&post.config.notes).take(shown) {
                println!("    {n}");
            }
        }
        assert!(!shaders.entries.is_empty(), "{}: no keys read", f.name);
        read += 1;
    }
    assert!(read > 0, "no packs under {}", dir.display());
}

/// Every module of `post` through the shader writers wgpu uses (SPIR-V for Vulkan, HLSL for
/// DirectX 12, GLSL for OpenGL, MSL for Metal), printing each error and panic (`BACKENDS=1`).
fn backends(post: &horizon_shaders::Post, dimension: &str) {
    let optional = [
        &post.terrain,
        &post.water,
        &post.sky,
        &post.sky_textured,
        &post.entities,
        &post.block,
        &post.entities_translucent,
        &post.particles,
        &post.clouds,
        &post.weather,
        &post.hand,
        &post.shadow,
        &post.shadow_water,
        &post.shadow_entities,
        &post.shadow_block,
    ];
    let mut modules: Vec<(String, &naga::Module)> = Vec::new();
    let mut programs: Vec<(&str, &naga::Module, &naga::Module)> = Vec::new();
    for p in post
        .programs
        .iter()
        .chain(&post.shadow_comp)
        .chain(optional.into_iter().flatten())
    {
        if let Some(program) = &p.program {
            modules.push((format!("{} vertex", p.name), &program.vertex));
            modules.push((format!("{} fragment", p.name), &program.fragment));
            programs.push((&p.name, &program.vertex, &program.fragment));
        }
        for (name, c) in &p.compute {
            modules.push((name.clone(), &c.module));
        }
    }
    let (mut panics, mut errors) = (0, 0);
    for (name, module) in &modules {
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(module)
        .expect("translated modules validate");
        let Some(entry) = module.entry_points.first() else {
            continue;
        };
        let stage = entry.stage;
        let point = entry.name.clone();
        let writers: [(&str, &dyn Fn() -> Result<(), String>); 4] = [
            ("spv", &|| {
                use naga::back::spv;
                // SPV_OUT=<folder>: also written there, as wgpu writes it for a Vulkan 1.3
                // Adreno (SPIR-V 1.6, no varying names, robust access in the driver), for
                // spirv-val.
                let out = std::env::var_os("SPV_OUT");
                let options = spv::Options {
                    lang_version: if out.is_some() { (1, 6) } else { (1, 3) },
                    flags: if out.is_some() {
                        spv::WriterFlags::FORCE_POINT_SIZE
                    } else {
                        spv::WriterFlags::LABEL_VARYINGS | spv::WriterFlags::FORCE_POINT_SIZE
                    },
                    bounds_check_policies: naga::proc::BoundsCheckPolicies {
                        index: naga::proc::BoundsCheckPolicy::Restrict,
                        buffer: if out.is_some() {
                            naga::proc::BoundsCheckPolicy::Unchecked
                        } else {
                            naga::proc::BoundsCheckPolicy::Restrict
                        },
                        image_load: if out.is_some() {
                            naga::proc::BoundsCheckPolicy::Unchecked
                        } else {
                            naga::proc::BoundsCheckPolicy::Restrict
                        },
                        binding_array: naga::proc::BoundsCheckPolicy::Unchecked,
                    },
                    force_loop_bounding: true,
                    emit_int_div_checks: true,
                    ..Default::default()
                };
                let pipeline = spv::PipelineOptions {
                    shader_stage: stage,
                    entry_point: point.clone(),
                };
                let words = spv::write_vec(module, &info, &options, Some(&pipeline))
                    .map_err(|e| e.to_string())?;
                if let Some(out) = &out {
                    let file: String = name
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect();
                    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                    std::fs::write(
                        Path::new(out).join(format!("{dimension}_{file}.spv")),
                        bytes,
                    )
                    .unwrap();
                }
                Ok(())
            }),
            ("hlsl", &|| {
                use naga::back::hlsl;
                let options = hlsl::Options {
                    shader_model: hlsl::ShaderModel::V6_0,
                    fake_missing_bindings: true,
                    ..Default::default()
                };
                let pipeline = hlsl::PipelineOptions {
                    entry_point: Some((stage, point.clone())),
                };
                let mut out = String::new();
                hlsl::Writer::new(&mut out, &options, &pipeline)
                    .write(module, &info, None)
                    .map(drop)
                    .map_err(|e| e.to_string())
            }),
            ("glsl", &|| {
                use naga::back::glsl;
                let options = glsl::Options {
                    version: glsl::Version::Desktop(430),
                    ..Default::default()
                };
                let pipeline = glsl::PipelineOptions {
                    shader_stage: stage,
                    entry_point: point.clone(),
                    multiview: None,
                };
                let mut out = String::new();
                glsl::Writer::new(
                    &mut out,
                    module,
                    &info,
                    &options,
                    &pipeline,
                    naga::proc::BoundsCheckPolicies::default(),
                )
                .and_then(|mut w| w.write().map(drop))
                .map_err(|e| e.to_string())
            }),
            ("msl", &|| {
                use naga::back::msl;
                let options = msl::Options {
                    lang_version: (2, 4),
                    fake_missing_bindings: true,
                    ..Default::default()
                };
                msl::write_string(module, &info, &options, &msl::PipelineOptions::default())
                    .map(drop)
                    .map_err(|e| e.to_string())
            }),
        ];
        for (backend, write) in writers {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(write)) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    errors += 1;
                    println!("    {backend} error: {name}: {e}");
                }
                Err(panic) => {
                    panics += 1;
                    let message = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default();
                    println!("    {backend} PANIC: {name}: {message}");
                }
            }
        }
    }
    println!(
        "    backends: {} modules, {errors} errors, {panics} panics",
        modules.len()
    );
    if std::env::var_os("EMPTIED").is_some() {
        emptied(&modules, &programs);
    }
    assert_eq!(
        (errors, panics),
        (0, 0),
        "a backend failed on a translated module"
    );
}

/// `EMPTIED=1`: every module with any one function left empty, as `narrow_down` tries a program
/// a GPU driver refuses, validates; and a driver refusing a scalar `distance` (what Lux's sky has
/// that none of its programs Adreno 740 compiles has) is narrowed down to the function using it.
fn emptied(modules: &[(String, &naga::Module)], programs: &[(&str, &naga::Module, &naga::Module)]) {
    let validate = |m: &naga::Module| {
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(m)
    };
    let (mut tried, mut invalid) = (0, 0);
    let start = Instant::now();
    for (name, module) in modules {
        for (f, function) in module.functions.iter() {
            tried += 1;
            if let Err(e) = validate(&horizon_shaders::emptied(module, f)) {
                invalid += 1;
                println!("    emptied {name}: {:?}: {e:?}", function.name);
            }
        }
    }
    println!(
        "    emptied: {tried} modules, {invalid} invalid, {:.2} ms each",
        start.elapsed().as_secs_f64() * 1000.0 / tried.max(1) as f64
    );
    assert_eq!(
        invalid, 0,
        "a module with a function left empty does not validate"
    );
    // Whether `m` writes a `distance` of two scalars (one in an expression nothing uses is not
    // written: compacting removes it, as emptying any function does).
    let scalar_distance = |m: &naga::Module| {
        let mut m = m.clone();
        naga::compact::compact(&mut m, naga::compact::KeepUnused::No);
        let m = &m;
        let info = validate(m).expect("validates");
        let functions = m.functions.iter().map(|(h, f)| (f, &info[h]));
        let entries = (m.entry_points.iter().enumerate())
            .map(|(i, e)| (&e.function, info.get_entry_point(i)));
        functions.chain(entries).any(|(f, fi)| {
            f.expressions.iter().any(|(_, e)| match *e {
                naga::Expression::Math {
                    fun: naga::MathFunction::Distance,
                    arg,
                    ..
                } => matches!(*fi[arg].ty.inner_with(&m.types), naga::TypeInner::Scalar(_)),
                _ => false,
            })
        })
    };
    for &(name, vertex, fragment) in programs {
        if !scalar_distance(vertex) && !scalar_distance(fragment) {
            continue;
        }
        let start = Instant::now();
        let note = horizon_shaders::narrow_down(
            vertex,
            fragment,
            |tried| tried < 24,
            |v, f| !scalar_distance(&v) && !scalar_distance(&f),
        );
        println!(
            "    {name}, a driver refusing scalar distance: {note}, {:.2} s",
            start.elapsed().as_secs_f64()
        );
    }
}
