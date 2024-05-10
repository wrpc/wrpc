use crate::interface::InterfaceGenerator;
use anyhow::Result;
use core::fmt::Display;
use heck::{ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::io::{Read, Write};
use std::mem;
use std::process::{Command, Stdio};
use wit_bindgen_core::{
    uwrite, uwriteln,
    wit_parser::{Function, InterfaceId, PackageId, Resolve, TypeId, World, WorldId, WorldKey},
    Files, InterfaceGenerator as _, Source, Types, WorldGenerator,
};

mod interface;

#[derive(Clone)]
struct InterfaceName {
    /// The string import name for this interface.
    import_name: String,

    /// The string import path for this interface.
    import_path: String,
}

#[derive(Default)]
struct GoWrpc {
    types: Types,
    src: Source,
    opts: Opts,
    import_modules: Vec<(String, Vec<String>)>,
    export_modules: Vec<(String, Vec<String>)>,
    skip: HashSet<String>,
    interface_names: HashMap<InterfaceId, InterfaceName>,
    /// Each imported and exported interface is stored in this map. Value indicates if last use was import.
    interface_last_seen_as_import: HashMap<InterfaceId, bool>,
    import_funcs_called: bool,
    world: Option<WorldId>,

    export_paths: Vec<String>,
    deps: Deps,
}

#[derive(Default)]
struct Deps {
    map: BTreeMap<String, String>,
    package: String,
}

impl Deps {
    fn binary(&mut self) -> &'static str {
        self.map
            .insert("binary".to_string(), "encoding/binary".to_string());
        "binary"
    }
    fn bytes(&mut self) -> &'static str {
        self.map.insert("bytes".to_string(), "bytes".to_string());
        "bytes"
    }

    fn context(&mut self) -> &'static str {
        self.map
            .insert("context".to_string(), "context".to_string());
        "context"
    }

    fn errors(&mut self) -> &'static str {
        self.map.insert("errors".to_string(), "errors".to_string());
        "errors"
    }

    fn fmt(&mut self) -> &'static str {
        self.map.insert("fmt".to_string(), "fmt".to_string());
        "fmt"
    }

    fn errgroup(&mut self) -> &'static str {
        self.map.insert(
            "errgroup".to_string(),
            "golang.org/x/sync/errgroup".to_string(),
        );
        "errgroup"
    }

    fn io(&mut self) -> &'static str {
        self.map.insert("io".to_string(), "io".to_string());
        "io"
    }

    fn math(&mut self) -> &'static str {
        self.map.insert("math".to_string(), "math".to_string());
        "math"
    }

    fn slog(&mut self) -> &'static str {
        self.map.insert("slog".to_string(), "log/slog".to_string());
        "slog"
    }

    fn strings(&mut self) -> &'static str {
        self.map
            .insert("strings".to_string(), "strings".to_string());
        "strings"
    }

    fn utf8(&mut self) -> &'static str {
        self.map
            .insert("utf8".to_string(), "unicode/utf8".to_string());
        "utf8"
    }

    fn wrpc(&mut self) -> &'static str {
        self.map
            .insert("wrpc".to_string(), "github.com/wrpc/wrpc/go".to_string());
        "wrpc"
    }

    fn import(&mut self, name: String, path: String) -> String {
        if let Some(old) = self.map.insert(name.clone(), path.clone()) {
            assert!(
                old == path,
                "dependency path mismatch, import of `{name}` refers to both `{old}` and `{path}`"
            );
        }
        name
    }
}

impl Display for Deps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "import (")?;
        for (k, v) in &self.map {
            writeln!(f, r#"{k} "{v}""#)?;
        }
        writeln!(f, ")")
    }
}

fn generated_preamble() -> String {
    format!(
        "// Generated by `wit-bindgen-wrpc-go` {}. DO NOT EDIT!\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct Opts {
    /// Whether or not `gofmt` is executed to format generated code.
    #[cfg_attr(feature = "clap", arg(long, default_missing_value = "true", default_value_t = true, num_args = 0..=1, require_equals = true, action = clap::ArgAction::Set))]
    pub gofmt: bool,

    /// Go package path containing the generated bindings
    #[cfg_attr(feature = "clap", arg(long, default_value = ""))]
    pub package: String,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            gofmt: true,
            package: String::new(),
        }
    }
}

impl Opts {
    #[must_use]
    pub fn build(self) -> Box<dyn WorldGenerator> {
        let mut r = GoWrpc::new();
        r.opts = self;
        r.deps.package.clone_from(&r.opts.package);
        Box::new(r)
    }
}

impl GoWrpc {
    fn new() -> GoWrpc {
        GoWrpc::default()
    }

    fn interface<'a>(
        &'a mut self,
        identifier: Identifier<'a>,
        resolve: &'a Resolve,
        in_import: bool,
    ) -> InterfaceGenerator<'a> {
        let package = self.opts.package.clone();
        InterfaceGenerator {
            identifier,
            src: Source::default(),
            in_import,
            gen: self,
            resolve,
            deps: Deps {
                map: BTreeMap::default(),
                package,
            },
        }
    }

    fn emit_modules(&mut self, modules: Vec<(String, Vec<String>)>, files: &mut Files) {
        for (mut module, path) in modules {
            if self.opts.gofmt {
                gofmt(&mut module);
            }
            let file = format!("{}/bindings.wrpc.go", path.join("/"));
            files.push(&file, generated_preamble().as_bytes());
            files.push(&file, module.as_bytes());
        }
    }

    fn name_interface(
        &mut self,
        resolve: &Resolve,
        id: InterfaceId,
        name: &WorldKey,
        is_export: bool,
    ) {
        let path = compute_module_path(name, resolve, is_export);
        let import_name = path.join("__");
        let import_path = if !self.opts.package.is_empty() {
            format!("{}/{}", self.opts.package, path.join("/"))
        } else {
            path.join("/")
        };
        self.interface_names.insert(
            id,
            InterfaceName {
                import_name,
                import_path,
            },
        );
    }

    /// Generates imports and a `Serve` function for the world
    fn finish_serve_function(&mut self) {
        let mut traits: Vec<String> = self
            .export_paths
            .iter()
            .map(|path| {
                if path.is_empty() {
                    "Handler".to_string()
                } else {
                    format!("{path}.Handler")
                }
            })
            .collect();
        let bound = match traits.len() {
            0 => return,
            1 => traits.pop().unwrap(),
            _ => traits.join("; "),
        };
        uwriteln!(
            self.src,
            r#"
func Serve(c {wrpc}.Client, h interface{{ {bound} }}) (stop func() error, err error) {{"#,
            wrpc = self.deps.wrpc()
        );
        uwriteln!(
            self.src,
            "stops := make([]func() error, 0, {})",
            self.export_paths.len()
        );
        self.src.push_str(
            r"stop = func() error {
            for _, stop := range stops {
                if err := stop(); err != nil {
                    return err
                }
            }
            return nil
        }
",
        );

        for (i, path) in self.export_paths.iter().enumerate() {
            uwrite!(self.src, "stop{i}, err := ");
            if !path.is_empty() {
                self.src.push_str(path);
                self.src.push_str(".");
            }
            self.src.push_str("ServeInterface(c, h)\n");
            self.src.push_str("if err != nil { return }\n");
            uwriteln!(self.src, "stops = append(stops, stop{i})");
        }
        self.src.push_str("stop = func() error {\n");
        for (i, _) in self.export_paths.iter().enumerate() {
            uwriteln!(self.src, "if err := stop{i}(); err != nil {{ return err }}");
        }
        self.src.push_str("return nil\n");
        self.src.push_str("}\n");

        self.src.push_str("return\n");
        self.src.push_str("}\n");
    }
}

/// If the package `id` is the only package with its namespace/name combo
/// then pass through the name unmodified. If, however, there are multiple
/// versions of this package then the package module is going to get version
/// information.
fn name_package_module(resolve: &Resolve, id: PackageId) -> String {
    let pkg = &resolve.packages[id];
    let versions_with_same_name = resolve
        .packages
        .iter()
        .filter_map(|(_, p)| {
            if p.name.namespace == pkg.name.namespace && p.name.name == pkg.name.name {
                Some(&p.name.version)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let base = pkg.name.name.to_snake_case();
    if versions_with_same_name.len() == 1 {
        return base;
    }

    let version = match &pkg.name.version {
        Some(version) => version,
        // If this package didn't have a version then don't mangle its name
        // and other packages with the same name but with versions present
        // will have their names mangled.
        None => return base,
    };

    // Here there's multiple packages with the same name that differ only in
    // version, so the version needs to be mangled into the Rust module name
    // that we're generating. This in theory could look at all of
    // `versions_with_same_name` and produce a minimal diff, e.g. for 0.1.0
    // and 0.2.0 this could generate "foo1" and "foo2", but for now
    // a simpler path is chosen to generate "foo0_1_0" and "foo0_2_0".
    let version = version
        .to_string()
        .replace(['.', '-', '+'], "_")
        .to_snake_case();
    format!("{base}{version}")
}

fn gofmt(src: &mut String) {
    let mut child = Command::new("gofmt")
        .args(["-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn `gofmt`");
    let buf = src.clone();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(src.as_bytes())
        .unwrap();
    src.truncate(0);
    child.stdout.take().unwrap().read_to_string(src).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "\n\n\n\n{buf}\n\n\n\n");
}

impl WorldGenerator for GoWrpc {
    fn preprocess(&mut self, resolve: &Resolve, world: WorldId) {
        self.types.analyze(resolve);
        self.world = Some(world);
    }

    fn import_interface(
        &mut self,
        resolve: &Resolve,
        name: &WorldKey,
        id: InterfaceId,
        _files: &mut Files,
    ) {
        self.interface_last_seen_as_import.insert(id, true);
        let mut gen = self.interface(Identifier::Interface(id, name), resolve, true);
        let (snake, module_path) = gen.start_append_submodule(name);
        gen.gen.name_interface(resolve, id, name, false);
        gen.types(id);

        let interface = &resolve.interfaces[id];
        let name = match name {
            WorldKey::Name(s) => s.to_string(),
            WorldKey::Interface(..) => interface
                .name
                .as_ref()
                .expect("interface name missing")
                .to_string(),
        };
        let instance = if let Some(package) = interface.package {
            resolve.id_of_name(package, &name)
        } else {
            name
        };
        gen.generate_imports(&instance, resolve.interfaces[id].functions.values());

        gen.finish_append_submodule(&snake, module_path);
    }

    fn import_funcs(
        &mut self,
        resolve: &Resolve,
        world: WorldId,
        funcs: &[(&str, &Function)],
        _files: &mut Files,
    ) {
        self.import_funcs_called = true;

        let mut gen = self.interface(Identifier::World(world), resolve, true);
        let World {
            ref name, package, ..
        } = resolve.worlds[world];
        let instance = if let Some(package) = package {
            resolve.id_of_name(package, name)
        } else {
            name.to_string()
        };
        gen.generate_imports(&instance, funcs.iter().map(|(_, func)| *func));

        let src = gen.finish();
        for (k, v) in gen.deps.map {
            self.deps.import(k, v);
        }
        self.src.push_str(&src);
    }

    fn export_interface(
        &mut self,
        resolve: &Resolve,
        name: &WorldKey,
        id: InterfaceId,
        _files: &mut Files,
    ) -> Result<()> {
        self.interface_last_seen_as_import.insert(id, false);
        let mut gen = self.interface(Identifier::Interface(id, name), resolve, false);
        let (snake, module_path) = gen.start_append_submodule(name);
        gen.gen.name_interface(resolve, id, name, true);
        gen.types(id);
        let exports = gen.generate_exports(
            Identifier::Interface(id, name),
            resolve.interfaces[id].functions.values(),
        );
        gen.finish_append_submodule(&snake, module_path);
        if exports {
            let InterfaceName {
                import_name,
                import_path,
            } = &self.interface_names[&id];
            self.export_paths
                .push(self.deps.import(import_name.clone(), import_path.clone()));
        }

        Ok(())
    }

    fn export_funcs(
        &mut self,
        resolve: &Resolve,
        world: WorldId,
        funcs: &[(&str, &Function)],
        _files: &mut Files,
    ) -> Result<()> {
        let mut gen = self.interface(Identifier::World(world), resolve, false);
        let exports = gen.generate_exports(Identifier::World(world), funcs.iter().map(|f| f.1));
        let src = gen.finish();
        for (k, v) in gen.deps.map {
            self.deps.import(k, v);
        }
        self.src.push_str(&src);
        if exports {
            self.export_paths.push(String::new());
        }
        Ok(())
    }

    fn import_types(
        &mut self,
        resolve: &Resolve,
        world: WorldId,
        types: &[(&str, TypeId)],
        _files: &mut Files,
    ) {
        let mut gen = self.interface(Identifier::World(world), resolve, true);
        for (name, ty) in types {
            gen.define_type(name, *ty);
        }
        let src = gen.finish();
        for (k, v) in gen.deps.map {
            self.deps.import(k, v);
        }
        self.src.push_str(&src);
    }

    fn finish_imports(&mut self, resolve: &Resolve, world: WorldId, files: &mut Files) {
        if !self.import_funcs_called {
            // We call `import_funcs` even if the world doesn't import any
            // functions since one of the side effects of that method is to
            // generate `struct`s for any imported resources.
            self.import_funcs(resolve, world, &[], files);
        }
    }

    fn finish(&mut self, resolve: &Resolve, world: WorldId, files: &mut Files) -> Result<()> {
        let import_modules = mem::take(&mut self.import_modules);
        self.emit_modules(import_modules, files);

        let export_modules = mem::take(&mut self.export_modules);
        self.emit_modules(export_modules, files);

        self.finish_serve_function();

        let mut src = mem::take(&mut self.src);
        let s = src.as_mut_string();

        let name = &resolve.worlds[world].name;
        let go_name = to_package_ident(name);
        *s = format!(
            r#"// {go_name} package contains wRPC bindings for `{name}` world
package {go_name}

{}

{s}"#,
            self.deps
        );
        if self.opts.gofmt {
            gofmt(src.as_mut_string());
        }
        let file = format!("{go_name}.wrpc.go");
        files.push(&file, generated_preamble().as_bytes());
        files.push(&file, src.as_bytes());
        Ok(())
    }
}

fn compute_module_path(name: &WorldKey, resolve: &Resolve, is_export: bool) -> Vec<String> {
    let mut path = Vec::new();
    if is_export {
        path.push("exports".to_string());
    }
    match name {
        WorldKey::Name(name) => {
            path.push(to_package_ident(name));
        }
        WorldKey::Interface(id) => {
            let iface = &resolve.interfaces[*id];
            let pkg = iface.package.unwrap();
            let pkgname = resolve.packages[pkg].name.clone();
            path.push(to_package_ident(&pkgname.namespace));
            path.push(name_package_module(resolve, pkg));
            path.push(to_package_ident(iface.name.as_ref().unwrap()));
        }
    }
    path
}

enum Identifier<'a> {
    World(WorldId),
    Interface(InterfaceId, &'a WorldKey),
}

#[derive(Default)]
struct FnSig {
    use_item_name: bool,
    self_arg: Option<String>,
    self_is_first_param: bool,
}

#[must_use]
pub fn to_package_ident(name: &str) -> String {
    match name {
        // Escape Go keywords.
        "break" => "break_".into(),
        "case" => "case_".into(),
        "chan" => "chan_".into(),
        "const" => "const_".into(),
        "continue" => "continue_".into(),
        "default" => "default_".into(),
        "defer" => "defer_".into(),
        "else" => "else_".into(),
        "enum" => "enum_".into(),
        "exports" => "exports_".into(),
        "fallthrough" => "fallthrough_".into(),
        "false" => "false_".into(),
        "for" => "for_".into(),
        "func" => "func_".into(),
        "go" => "go_".into(),
        "goto" => "goto_".into(),
        "if" => "if_".into(),
        "import" => "import_".into(),
        "interface" => "interface_".into(),
        "map" => "map_".into(),
        "package" => "package_".into(),
        "range" => "range_".into(),
        "return" => "return_".into(),
        "select" => "select_".into(),
        "struct" => "struct_".into(),
        "switch" => "switch_".into(),
        "true" => "true_".into(),
        "type" => "type_".into(),
        "var" => "var_".into(),
        s => s.to_snake_case(),
    }
}

#[must_use]
pub fn to_go_ident(name: &str) -> String {
    match name {
        // Escape Go keywords.
        "break" => "break_".into(),
        "case" => "case_".into(),
        "chan" => "chan_".into(),
        "const" => "const_".into(),
        "continue" => "continue_".into(),
        "default" => "default_".into(),
        "defer" => "defer_".into(),
        "else" => "else_".into(),
        "enum" => "enum_".into(),
        "fallthrough" => "fallthrough_".into(),
        "false" => "false_".into(),
        "for" => "for_".into(),
        "func" => "func_".into(),
        "go" => "go_".into(),
        "goto" => "goto_".into(),
        "if" => "if_".into(),
        "import" => "import_".into(),
        "interface" => "interface_".into(),
        "map" => "map_".into(),
        "package" => "package_".into(),
        "range" => "range_".into(),
        "return" => "return_".into(),
        "select" => "select_".into(),
        "struct" => "struct_".into(),
        "switch" => "switch_".into(),
        "true" => "true_".into(),
        "type" => "type_".into(),
        "var" => "var_".into(),
        s => s.to_lower_camel_case(),
    }
}

fn to_upper_camel_case(name: &str) -> String {
    match name {
        // The name "Handler" is reserved for traits generated by exported
        // interfaces, so remap types defined in wit to something else.
        "handler" => "Handler_".to_string(),
        s => s.to_upper_camel_case(),
    }
}
