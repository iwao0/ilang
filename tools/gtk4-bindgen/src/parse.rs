//! Streaming GIR XML reader.
//!
//! Walks the file once with `quick-xml::Reader`, keeping a small
//! element stack to track which container we're currently inside.
//! Anything we don't recognise gets skipped — first-cut goal is a
//! useful subset, not a 100% faithful round-trip.

use std::collections::HashSet;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::Reader;

use crate::types::{
    ClassDef, EnumDef, EnumMember, FunDef, Namespace, Param, ParamDir, RecordDef,
    Repo, TypeRef, map_primitive,
};

pub fn parse_gir(path: &Path, repo: &mut Repo) -> Result<(), String> {
    let mut reader = Reader::from_file(path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut ns: Option<Namespace> = None;
    // Element stack — pushed/popped by Start / End events. We track
    // only the tags we care about; `<doc>` / `<source-position>` etc.
    // get pushed too but the handlers ignore them.
    let mut stack: Vec<String> = Vec::new();
    // Mutable "current" buckets while we're inside one of these.
    let mut cur_class: Option<ClassDef> = None;
    let mut cur_enum: Option<EnumDef> = None;
    let mut cur_fn: Option<FunDef> = None;
    let mut cur_param: Option<Param> = None;
    // When we enter <return-value> or <parameter> we set this to know
    // where to drop the <type>'s captured info.
    let mut type_target: Option<TypeTarget> = None;
    // Inside <parameters>, we either own a `<parameter>` or the
    // instance one. Track which.
    let mut in_instance_param = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(format!("xml: {e}")),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let tag = local_name(e.name());
                match tag.as_str() {
                    "namespace" => {
                        let name = attr(&e, b"name").unwrap_or_default();
                        ns = Some(Namespace {
                            name,
                            enums: Vec::new(),
                            classes: Vec::new(),
                            funcs: Vec::new(),
                            records: Vec::new(),
                        });
                    }
                    "class" => {
                        let c_type = attr(&e, b"type")
                            .or_else(|| attr(&e, b"type-name"))
                            .unwrap_or_default();
                        cur_class = Some(ClassDef {
                            name: attr(&e, b"name").unwrap_or_default(),
                            c_type,
                            constructors: Vec::new(),
                            methods: Vec::new(),
                            funcs: Vec::new(),
                            deprecated: is_deprecated(&e),
                        });
                    }
                    "record" => {
                        if let Some(n) = ns.as_mut() {
                            let name = attr(&e, b"name").unwrap_or_default();
                            let c_type = attr(&e, b"type").unwrap_or_default();
                            // GIR `<record>` covers anonymous helper
                            // structs too; skip the boring ones.
                            if !c_type.is_empty() && !name.is_empty() {
                                n.records.push(RecordDef {
                                    name,
                                    c_type,
                                    deprecated: is_deprecated(&e),
                                });
                            }
                        }
                    }
                    "enumeration" | "bitfield" => {
                        cur_enum = Some(EnumDef {
                            name: attr(&e, b"name").unwrap_or_default(),
                            c_type: attr(&e, b"type").unwrap_or_default(),
                            is_flags: tag == "bitfield",
                            members: Vec::new(),
                            deprecated: is_deprecated(&e),
                        });
                    }
                    "constructor" | "method" | "function" => {
                        cur_fn = Some(FunDef {
                            name: attr(&e, b"name").unwrap_or_default(),
                            c_identifier: attr(&e, b"identifier")
                                .unwrap_or_default(),
                            params: Vec::new(),
                            return_type: None,
                            deprecated: is_deprecated(&e),
                            variadic: false,
                            throws: attr(&e, b"throws")
                                .as_deref()
                                == Some("1"),
                            instance: None,
                        });
                    }
                    "parameter" => {
                        cur_param = Some(Param {
                            name: attr(&e, b"name").unwrap_or_default(),
                            ty: TypeRef::Unsupported("unset".into()),
                            direction: match attr(&e, b"direction").as_deref() {
                                Some("out") => ParamDir::Out,
                                Some("inout") => ParamDir::InOut,
                                _ => ParamDir::In,
                            },
                        });
                        type_target = Some(TypeTarget::Param);
                        in_instance_param = false;
                    }
                    "instance-parameter" => {
                        cur_param = Some(Param {
                            name: attr(&e, b"name").unwrap_or_default(),
                            ty: TypeRef::Unsupported("unset".into()),
                            direction: ParamDir::In,
                        });
                        type_target = Some(TypeTarget::Param);
                        in_instance_param = true;
                    }
                    "return-value" => {
                        type_target = Some(TypeTarget::Return);
                    }
                    "array" => {
                        // Mark whichever slot is open as unsupported
                        // — first cut doesn't emit array params.
                        bump_type(
                            &mut type_target,
                            &mut cur_param,
                            &mut cur_fn,
                            TypeRef::Unsupported("array".into()),
                        );
                    }
                    "member" => {
                        // GIR `<member>` elements wrap `<doc>` so
                        // they arrive as Start, not Empty. Capture
                        // the attributes the same way.
                        if let Some(en) = cur_enum.as_mut() {
                            let name = attr(&e, b"name").unwrap_or_default();
                            let val = attr(&e, b"value")
                                .and_then(|s| s.parse::<i64>().ok())
                                .unwrap_or(0);
                            en.members.push(EnumMember { name, value: val });
                        }
                    }
                    _ => {}
                }
                stack.push(tag);
            }
            Ok(Event::Empty(e)) => {
                let tag = local_name(e.name());
                match tag.as_str() {
                    "type" => {
                        if let Some(name) = attr(&e, b"name") {
                            let t = if let Some(p) = map_primitive(&name) {
                                p
                            } else if name == "GLib.Error" {
                                TypeRef::Unsupported("GLib.Error".into())
                            } else {
                                // Unresolved-named — caller phase
                                // upgrades to `Named(...)` once
                                // every namespace is loaded. Until
                                // then, store the raw GIR name.
                                TypeRef::Named(qualify(&name, ns.as_ref()))
                            };
                            bump_type(
                                &mut type_target,
                                &mut cur_param,
                                &mut cur_fn,
                                t,
                            );
                        }
                    }
                    "varargs" => {
                        if let Some(f) = cur_fn.as_mut() {
                            f.variadic = true;
                        }
                    }
                    "member" => {
                        // Inside <enumeration> / <bitfield>.
                        if let Some(en) = cur_enum.as_mut() {
                            let name = attr(&e, b"name").unwrap_or_default();
                            let val = attr(&e, b"value")
                                .and_then(|s| s.parse::<i64>().ok())
                                .unwrap_or(0);
                            en.members.push(EnumMember { name, value: val });
                        }
                    }
                    "array" => {
                        bump_type(
                            &mut type_target,
                            &mut cur_param,
                            &mut cur_fn,
                            TypeRef::Unsupported("array".into()),
                        );
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let tag = local_name(e.name());
                let _ = stack.pop();
                match tag.as_str() {
                    "namespace" => {
                        if let Some(n) = ns.take() {
                            repo.namespaces.push(n);
                        }
                    }
                    "class" => {
                        if let (Some(n), Some(c)) =
                            (ns.as_mut(), cur_class.take())
                        {
                            n.classes.push(c);
                        }
                    }
                    "enumeration" | "bitfield" => {
                        if let (Some(n), Some(en)) =
                            (ns.as_mut(), cur_enum.take())
                        {
                            n.enums.push(en);
                        }
                    }
                    "constructor" | "method" | "function" => {
                        let kind = tag;
                        if let Some(f) = cur_fn.take() {
                            place_fn(kind.as_str(), f, cur_class.as_mut(), ns.as_mut());
                        }
                    }
                    "parameter" | "instance-parameter" => {
                        if let (Some(p), Some(f)) =
                            (cur_param.take(), cur_fn.as_mut())
                        {
                            if in_instance_param {
                                f.instance = Some(p.ty);
                            } else {
                                f.params.push(p);
                            }
                        }
                        type_target = None;
                        in_instance_param = false;
                    }
                    "return-value" => {
                        type_target = None;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum TypeTarget {
    Param,
    Return,
}

fn bump_type(
    target: &mut Option<TypeTarget>,
    cur_param: &mut Option<Param>,
    cur_fn: &mut Option<FunDef>,
    t: TypeRef,
) {
    match target {
        Some(TypeTarget::Param) => {
            if let Some(p) = cur_param.as_mut() {
                p.ty = t;
            }
        }
        Some(TypeTarget::Return) => {
            if let Some(f) = cur_fn.as_mut() {
                f.return_type = Some(t);
            }
        }
        None => {}
    }
}

fn place_fn(
    kind: &str,
    f: FunDef,
    cur_class: Option<&mut ClassDef>,
    ns: Option<&mut Namespace>,
) {
    match (kind, cur_class, ns) {
        ("constructor", Some(c), _) => c.constructors.push(f),
        ("method", Some(c), _) => c.methods.push(f),
        ("function", Some(c), _) => c.funcs.push(f),
        ("function", None, Some(n)) => n.funcs.push(f),
        _ => {}
    }
}

fn local_name(qn: QName<'_>) -> String {
    let s = std::str::from_utf8(qn.local_name().into_inner()).unwrap_or("");
    s.to_string()
}

fn attr(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        let local = a.key.local_name();
        if local.into_inner() == key {
            return Some(
                std::str::from_utf8(&a.value).unwrap_or("").to_string(),
            );
        }
    }
    None
}

fn is_deprecated(e: &BytesStart<'_>) -> bool {
    attr(e, b"deprecated").as_deref() == Some("1")
}

fn qualify(name: &str, ns: Option<&Namespace>) -> String {
    if name.contains('.') {
        name.to_string()
    } else if let Some(n) = ns {
        format!("{}.{}", n.name, name)
    } else {
        name.to_string()
    }
}

/// Build the set of all `<NS>.<Name>` identifiers known across the
/// loaded repos. Lets the emitter filter out fns that reference
/// types we haven't pulled in.
pub fn known_type_set(repo: &Repo) -> HashSet<String> {
    let mut s = HashSet::new();
    for n in &repo.namespaces {
        for c in &n.classes {
            s.insert(format!("{}.{}", n.name, c.name));
        }
        for r in &n.records {
            s.insert(format!("{}.{}", n.name, r.name));
        }
        for e in &n.enums {
            s.insert(format!("{}.{}", n.name, e.name));
        }
    }
    s
}

/// Build `<NS>.<Name>` → C-side type name map. Used by the emitter
/// to render type references as their globally-unique C names
/// (so cross-namespace duplicates like `Gdk.AppLaunchContext` and
/// `Gio.AppLaunchContext` don't both turn into `AppLaunchContext`).
pub fn c_type_map(repo: &Repo) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for n in &repo.namespaces {
        for c in &n.classes {
            out.insert(
                format!("{}.{}", n.name, c.name),
                if c.c_type.is_empty() { c.name.clone() } else { c.c_type.clone() },
            );
        }
        for r in &n.records {
            out.insert(
                format!("{}.{}", n.name, r.name),
                if r.c_type.is_empty() { r.name.clone() } else { r.c_type.clone() },
            );
        }
        for e in &n.enums {
            out.insert(
                format!("{}.{}", n.name, e.name),
                if e.c_type.is_empty() { e.name.clone() } else { e.c_type.clone() },
            );
        }
    }
    out
}
