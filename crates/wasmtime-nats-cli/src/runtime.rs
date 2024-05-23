use core::iter::zip;

use std::sync::Arc;

use anyhow::Context as _;
use tracing::{error, instrument, trace, warn};
use wasmtime::component::{types, Linker};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiView};
use wrpc_runtime_wasmtime::{from_wrpc_value, to_wrpc_value};
use wrpc_types::DynamicFunction;

pub struct Ctx<C> {
    pub ctx: WasiCtx,
    pub table: ResourceTable,
    pub wrpc: C,
}

impl<C: Send> WasiView for Ctx<C> {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

/// Polyfills all missing imports
#[instrument(level = "trace", skip_all)]
pub fn polyfill<'a, T, C>(
    resolve: &wit_parser::Resolve,
    imports: T,
    engine: &wasmtime::Engine,
    ty: &types::Component,
    linker: &mut Linker<Ctx<C>>,
) where
    T: IntoIterator<Item = (&'a wit_parser::WorldKey, &'a wit_parser::WorldItem)>,
    T::IntoIter: ExactSizeIterator,
    C: wrpc_transport::Client + Send,
{
    let imports = imports.into_iter();
    for (wk, item) in imports {
        let instance_name = resolve.name_world_key(wk);
        // Avoid polyfilling instances, for which static bindings are linked
        match instance_name.as_ref() {
            "wasi:cli/environment@0.2.0"
            | "wasi:cli/exit@0.2.0"
            | "wasi:cli/stderr@0.2.0"
            | "wasi:cli/stdin@0.2.0"
            | "wasi:cli/stdout@0.2.0"
            | "wasi:cli/terminal-input@0.2.0"
            | "wasi:cli/terminal-output@0.2.0"
            | "wasi:cli/terminal-stderr@0.2.0"
            | "wasi:cli/terminal-stdin@0.2.0"
            | "wasi:cli/terminal-stdout@0.2.0"
            | "wasi:clocks/monotonic-clock@0.2.0"
            | "wasi:clocks/wall-clock@0.2.0"
            | "wasi:filesystem/preopens@0.2.0"
            | "wasi:filesystem/types@0.2.0"
            | "wasi:http/incoming-handler@0.2.0"
            | "wasi:http/outgoing-handler@0.2.0"
            | "wasi:http/types@0.2.0"
            | "wasi:io/error@0.2.0"
            | "wasi:io/poll@0.2.0"
            | "wasi:io/streams@0.2.0"
            | "wasi:keyvalue/store@0.2.0-draft"
            | "wasi:random/random@0.2.0"
            | "wasi:sockets/instance-network@0.2.0"
            | "wasi:sockets/network@0.2.0"
            | "wasi:sockets/tcp-create-socket@0.2.0"
            | "wasi:sockets/tcp@0.2.0"
            | "wasi:sockets/udp-create-socket@0.2.0"
            | "wasi:sockets/udp@0.2.0" => continue,
            _ => {}
        }
        let wit_parser::WorldItem::Interface(interface) = item else {
            continue;
        };
        let Some(wit_parser::Interface { functions, .. }) = resolve.interfaces.get(*interface)
        else {
            warn!("component imports a non-existent interface");
            continue;
        };
        let Some(types::ComponentItem::ComponentInstance(instance)) =
            ty.get_import(engine, &instance_name)
        else {
            trace!(
                instance_name,
                "component does not import the parsed instance"
            );
            continue;
        };

        let mut linker = linker.root();
        let mut linker = match linker.instance(&instance_name) {
            Ok(linker) => linker,
            Err(err) => {
                error!(
                    ?err,
                    ?instance_name,
                    "failed to instantiate interface from root"
                );
                continue;
            }
        };
        let instance_name = Arc::new(instance_name);
        for (func_name, ty) in functions {
            trace!(
                ?instance_name,
                func_name,
                "polyfill component function import"
            );
            let ty = match DynamicFunction::resolve(resolve, ty) {
                Ok(ty) => ty,
                Err(err) => {
                    error!(?err, "failed to resolve polyfilled function type");
                    continue;
                }
            };
            let result_ty = match ty {
                DynamicFunction::Method { results, .. } => Arc::clone(&results),
                DynamicFunction::Static { results, .. } => Arc::clone(&results),
            };
            let Some(types::ComponentItem::ComponentFunc(func)) =
                instance.get_export(engine, func_name)
            else {
                trace!(
                    ?instance_name,
                    func_name,
                    "instance does not export the parsed function"
                );
                continue;
            };
            let instance_name = Arc::clone(&instance_name);
            let func_name = Arc::new(func_name.to_string());
            if let Err(err) = linker.func_new_async(
                Arc::clone(&func_name).as_str(),
                move |mut store, params, results| {
                    let instance_name = Arc::clone(&instance_name);
                    let func_name = Arc::clone(&func_name);
                    let result_ty = Arc::clone(&result_ty);
                    let func = func.clone();
                    Box::new(async move {
                        let params: Vec<_> = zip(params, func.params())
                            .map(|(val, ty)| to_wrpc_value(&mut store, val, &ty))
                            .collect::<anyhow::Result<_>>()
                            .context("failed to convert wasmtime values to wRPC values")?;
                        let (result_values, tx) = store
                            .data()
                            .wrpc
                            .invoke_dynamic(&instance_name, &func_name, params, &result_ty)
                            .await
                            .with_context(|| {
                                format!("failed to invoke `{instance_name}.{func_name}` polyfill via wRPC")
                            })?;
                        for (i, (val, ty)) in zip(result_values, func.results()).enumerate() {
                            let val = from_wrpc_value(&mut store, val, &ty)?;
                            let result = results.get_mut(i).context("invalid result vector")?;
                            *result = val;
                        }
                        tx.await.context("failed to transmit parameters")?;
                        Ok(())
                    })
                },
            ) {
                error!(?err, "failed to polyfill component function import");
            }
        }
    }
}
