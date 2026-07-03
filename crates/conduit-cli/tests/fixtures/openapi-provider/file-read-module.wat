(module
  (import "cm32p2|conduit:plugin/file-read-v1" "read-text"
    (func $read-text (param i32 i32 i32)))

  (memory (export "cm32p2_memory") 1)
  (global $heap (mut i32) (i32.const 2048))

  (data (i32.const 16) "fixture-openapi")
  (data (i32.const 32) "0.1.0")
  (data (i32.const 40) "1")
  (data (i32.const 48) "openapi-provider-v1")
  (data (i32.const 80) "fixture-service")
  (data (i32.const 96) "GET")
  (data (i32.const 112) "fileReadOperation")

  (func (export "cm32p2|conduit:plugin/metadata|metadata") (result i32)
    ;; providers[0] = "openapi-provider-v1"
    i32.const 160
    i32.const 48
    i32.store
    i32.const 164
    i32.const 19
    i32.store

    ;; plugin-metadata record
    i32.const 200
    i32.const 16
    i32.store
    i32.const 204
    i32.const 15
    i32.store
    i32.const 208
    i32.const 32
    i32.store
    i32.const 212
    i32.const 5
    i32.store
    i32.const 216
    i32.const 40
    i32.store
    i32.const 220
    i32.const 1
    i32.store
    i32.const 224
    i32.const 160
    i32.store
    i32.const 228
    i32.const 1
    i32.store
    i32.const 200
  )

  (func (export "cm32p2|conduit:plugin/metadata|metadata_post") (param i32))

  (func (export "cm32p2|conduit:plugin/openapi-provider-v1|get-operation")
    (param $service-ptr i32)
    (param $service-len i32)
    (param $environment-tag i32)
    (param $environment-ptr i32)
    (param $environment-len i32)
    (param $method-tag i32)
    (param $method-ptr i32)
    (param $method-len i32)
    (param $path-tag i32)
    (param $path-ptr i32)
    (param $path-len i32)
    (result i32)

    ;; file-read result area starts at 512.
    local.get $path-ptr
    local.get $path-len
    i32.const 512
    call $read-text

    ;; result discriminant: ok
    i32.const 256
    i32.const 0
    i32.store

    ;; operation record payload starts at 260.
    i32.const 260
    i32.const 80
    i32.store
    i32.const 264
    i32.const 15
    i32.store

    ;; environment: none
    i32.const 268
    i32.const 0
    i32.store

    i32.const 280
    i32.const 96
    i32.store
    i32.const 284
    i32.const 3
    i32.store
    i32.const 288
    local.get $path-ptr
    i32.store
    i32.const 292
    local.get $path-len
    i32.store

    ;; parameters: empty list
    i32.const 296
    i32.const 0
    i32.store
    i32.const 300
    i32.const 0
    i32.store

    ;; operation-id: some("fileReadOperation")
    i32.const 304
    i32.const 1
    i32.store
    i32.const 308
    i32.const 112
    i32.store
    i32.const 312
    i32.const 17
    i32.store

    ;; summary: some(read-text ok string)
    i32.const 316
    i32.const 512
    i32.load
    i32.eqz
    i32.store
    i32.const 320
    i32.const 516
    i32.load
    i32.store
    i32.const 324
    i32.const 520
    i32.load
    i32.store

    ;; description/request-schema-json/response-schema-json/source: none
    i32.const 328
    i32.const 0
    i32.store
    i32.const 340
    i32.const 0
    i32.store
    i32.const 352
    i32.const 0
    i32.store
    i32.const 364
    i32.const 0
    i32.store

    i32.const 256
  )

  (func (export "cm32p2|conduit:plugin/openapi-provider-v1|get-operation_post") (param i32))

  (func (export "cm32p2|conduit:plugin/openapi-provider-v1|operations")
    (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)
    (result i32)
    ;; result discriminant: ok, payload: empty list
    i32.const 384
    i32.const 0
    i32.store
    i32.const 388
    i32.const 0
    i32.store
    i32.const 392
    i32.const 0
    i32.store
    i32.const 384
  )

  (func (export "cm32p2|conduit:plugin/openapi-provider-v1|operations_post") (param i32))

  (func (export "cm32p2_realloc")
    (param $ptr i32)
    (param $old-size i32)
    (param $align i32)
    (param $new-size i32)
    (result i32)
    (local $result i32)

    global.get $heap
    local.get $align
    i32.const 1
    i32.sub
    i32.add
    local.get $align
    i32.const 1
    i32.sub
    i32.const -1
    i32.xor
    i32.and
    local.tee $result

    local.get $new-size
    i32.add
    global.set $heap

    local.get $result
  )

  (func (export "cm32p2_initialize"))
)
