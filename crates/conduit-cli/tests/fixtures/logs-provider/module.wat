(module
  (memory (export "cm32p2_memory") 1)

  ;; Static fixture data and records live below 2048. Guest realloc starts at
  ;; 2048 so lowered request strings cannot overwrite the returned values.
  (global $heap (mut i32) (i32.const 2048))

  (data (i32.const 16) "fixture-logs")
  (data (i32.const 32) "0.1.0")
  (data (i32.const 40) "1")
  (data (i32.const 48) "logs-provider-v1")
  (data (i32.const 600) "fixture-service")
  (data (i32.const 620) "staging")
  (data (i32.const 640) "2026-05-22T10:00:00Z")
  (data (i32.const 672) "2026-05-22T10:15:00Z")
  (data (i32.const 704) "since 15m")
  (data (i32.const 720) "fixture://logs/auth")
  (data (i32.const 752) "auth_stored")
  (data (i32.const 768) "stored by fixture")
  (data (i32.const 800) "ACCOUNT_NOT_ACTIVATED")

  (func (export "cm32p2|conduit:plugin/metadata|metadata") (result i32)
    ;; providers[0] = "logs-provider-v1"
    (i32.store (i32.const 160) (i32.const 48))
    (i32.store (i32.const 164) (i32.const 16))

    ;; plugin-metadata record
    (i32.store (i32.const 200) (i32.const 16))
    (i32.store (i32.const 204) (i32.const 12))
    (i32.store (i32.const 208) (i32.const 32))
    (i32.store (i32.const 212) (i32.const 5))
    (i32.store (i32.const 216) (i32.const 40))
    (i32.store (i32.const 220) (i32.const 1))
    (i32.store (i32.const 224) (i32.const 160))
    (i32.store (i32.const 228) (i32.const 1))
    i32.const 200
  )

  (func (export "cm32p2|conduit:plugin/metadata|metadata_post") (param i32))

  (func (export "cm32p2|conduit:plugin/logs-provider-v1|search")
    (param i32)
    (result i32)
    ;; result<search-result, provider-error>. The runtime test verifies that
    ;; Wasmtime can lift the success shape and its nested log-event list.
    (i32.store (i32.const 256) (i32.const 0))

    ;; search-result payload starts at result + 8.
    (i32.store (i32.const 264) (i32.const 0)) ;; status = ok
    (i32.store (i32.const 268) (i32.const 16)) ;; provider.ptr
    (i32.store (i32.const 272) (i32.const 12)) ;; provider.len
    (i32.store (i32.const 276) (i32.const 600)) ;; service.ptr
    (i32.store (i32.const 280) (i32.const 15)) ;; service.len
    (i32.store (i32.const 284) (i32.const 1)) ;; environment = some
    (i32.store (i32.const 288) (i32.const 620)) ;; environment.ptr
    (i32.store (i32.const 292) (i32.const 7)) ;; environment.len

    ;; time-range
    (i32.store (i32.const 296) (i32.const 640)) ;; from.ptr
    (i32.store (i32.const 300) (i32.const 20)) ;; from.len
    (i32.store (i32.const 304) (i32.const 1)) ;; to = some
    (i32.store (i32.const 308) (i32.const 672)) ;; to.ptr
    (i32.store (i32.const 312) (i32.const 20)) ;; to.len
    (i32.store (i32.const 316) (i32.const 704)) ;; source.ptr
    (i32.store (i32.const 320) (i32.const 9)) ;; source.len

    (i32.store (i32.const 328) (i32.const 1)) ;; matches = some
    (i64.store (i32.const 336) (i64.const 1))
    (i64.store (i32.const 344) (i64.const 1)) ;; shown
    (i32.store (i32.const 352) (i32.const 928)) ;; logs.ptr
    (i32.store (i32.const 356) (i32.const 1)) ;; logs.len
    (i32.store (i32.const 360) (i32.const 0)) ;; next-cursor = none
    (i32.store (i32.const 372) (i32.const 1)) ;; checked-until = some
    (i32.store (i32.const 376) (i32.const 672)) ;; checked-until.ptr
    (i32.store (i32.const 380) (i32.const 20)) ;; checked-until.len
    (i32.store (i32.const 384) (i32.const 0)) ;; diagnostics.ptr
    (i32.store (i32.const 388) (i32.const 0)) ;; diagnostics.len

    ;; log-event
    (i32.store (i32.const 928) (i32.const 0)) ;; id = none
    (i32.store (i32.const 940) (i32.const 640)) ;; timestamp.ptr
    (i32.store (i32.const 944) (i32.const 20)) ;; timestamp.len
    (i32.store (i32.const 948) (i32.const 0)) ;; level = none
    (i32.store (i32.const 960) (i32.const 0)) ;; service = none
    (i32.store (i32.const 972) (i32.const 0)) ;; environment = none
    (i32.store (i32.const 984) (i32.const 0)) ;; cid = none
    (i32.store (i32.const 996) (i32.const 0)) ;; trace-id = none
    (i32.store (i32.const 1008) (i32.const 0)) ;; logger = none
    (i32.store (i32.const 1020) (i32.const 800)) ;; message.ptr
    (i32.store (i32.const 1024) (i32.const 21)) ;; message.len
    (i32.store (i32.const 1028) (i32.const 0)) ;; stack-trace = none
    (i32.store (i32.const 1040) (i32.const 0)) ;; source = none
    (i32.store (i32.const 1052) (i32.const 0)) ;; attributes-json = none

    i32.const 256
  )

  (func (export "cm32p2|conduit:plugin/logs-provider-v1|search_post") (param i32))

  (func (export "cm32p2|conduit:plugin/logs-provider-v1|authenticate")
    (param i32 i32 i32 i32 i32 i32 i32)
    (result i32)
    ;; result<auth-result, provider-error>
    (i32.store (i32.const 448) (i32.const 0))

    ;; auth-result payload starts at result + 4.
    (i32.store (i32.const 452) (i32.const 0)) ;; status = ok
    (i32.store (i32.const 456) (i32.const 16)) ;; provider.ptr
    (i32.store (i32.const 460) (i32.const 12)) ;; provider.len
    (i32.store (i32.const 464) (i32.const 1)) ;; environment = some
    (i32.store (i32.const 468) (i32.const 620)) ;; environment.ptr
    (i32.store (i32.const 472) (i32.const 7)) ;; environment.len
    (i32.store (i32.const 476) (i32.const 1)) ;; destination = some
    (i32.store (i32.const 480) (i32.const 720)) ;; destination.ptr
    (i32.store (i32.const 484) (i32.const 19)) ;; destination.len
    (i32.store (i32.const 488) (i32.const 0)) ;; expires-at = none
    (i32.store (i32.const 500) (i32.const 512)) ;; diagnostics.ptr
    (i32.store (i32.const 504) (i32.const 1)) ;; diagnostics.len

    ;; diagnostic
    (i32.store (i32.const 512) (i32.const 752)) ;; kind.ptr
    (i32.store (i32.const 516) (i32.const 11)) ;; kind.len
    (i32.store (i32.const 520) (i32.const 1)) ;; hint = some
    (i32.store (i32.const 524) (i32.const 768)) ;; hint.ptr
    (i32.store (i32.const 528) (i32.const 17)) ;; hint.len

    i32.const 448
  )

  (func (export "cm32p2|conduit:plugin/logs-provider-v1|authenticate_post") (param i32))

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
