pub(crate) mod openapi {
    wasmtime::component::bindgen!({
        path: "../../wit/conduit-plugin",
        world: "openapi-provider",
    });
}

pub(crate) mod logs {
    wasmtime::component::bindgen!({
        path: "../../wit/conduit-plugin",
        world: "logs-provider",
    });
}

pub(crate) mod db {
    wasmtime::component::bindgen!({
        path: "../../wit/conduit-plugin",
        world: "db-provider",
    });
}
