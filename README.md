# fezz

A single Rust-based Host HTTP Runtime (HHRF) runs lightweight "fetch-like" Fezz function modules using a load-run-unload execution model, providing a serverless-style platform.

## Architecture

### Process-Based Execution (fezz-runner)

HHRF artık fonksiyonları aynı process içinde `libloading` ile çalıştırmak yerine, her çağrıyı ayrı bir **child process** üzerinden yürütür:

- HHRF, manifest'ten `.dylib`/`.so` yolunu okur.
- `fezz-runner` binary'sini spawn eder ve `.dylib` yolunu argüman olarak geçirir.
- HTTP isteğini `FezzWireRequest` olarak CBOR bytes'a çevirip child process'in `stdin`'ine yazar.
- Child process içindeki `fezz-runner`, ilgili library'yi yükler, `fezz_handle_v2` fonksiyonunu çağırır ve dönen `FezzWireResponse` bytes'ını `stdout`'a yazar.
- HHRF, `stdout`'tan bu bytes'ı okuyup gerçek HTTP cevabına dönüştürür.

Bu model sayesinde user kodu HHRF'den ayrı bir process olarak çalışır; panik / segfault HHRF'yi düşürmez ve ileride OS-level jailer ile izole etmek kolaylaşır.

### Runner'ı Jail ile Sarmak

HHRF, kullanacağı runner binary'sini `FEZZ_RUNNER` ortam değişkeni ile ayarlamana izin verir:

- Varsayılan: `FEZZ_RUNNER` tanımlı değilse `fezz-runner` kullanılır.
- Prod ortamda Linux üzerinde, `FEZZ_RUNNER`'ı bir jailer ile wrap edebilirsin (ör. `nsjail`, `firejail`, `bwrap`).

Örnek (konsept):

```bash
export FEZZ_RUNNER=/usr/local/bin/nsjail-fezz-runner
```

`nsjail-fezz-runner` script/bin'i içinde:

- Low-priv user'a geç,
- Gerekirse chroot / namespace / seccomp ayarlarını yap,
- Sonra gerçek `fezz-runner`'ı bu sandbox içinde çalıştır.

### Panic Safety

`#[fezz_function]` macro'su, user fonksiyonunu `std::panic::catch_unwind` ile saran bir `fezz_handle_v2` FFI entrypoint'i üretir. Böylece user kodundaki panikler FFI boundary'yi geçmez, HTTP 500 dönen structured error response'a çevrilir.

### Async Runtime Isolation

`fezz_handle_v2` exported C fonksiyonu senkron çalışır. Fonksiyon içinde HTTP/Redis gibi işler için bloklayan client'lar kullanılır. HHRF tarafında çağrı artık ayrı bir process olduğu için host async runtime'ı bloklanmaz; ek olarak process-level izolasyon kazanılır.

## Best Practices for User Functions

### State and Caching

Artık her çağrı ayrı bir process içinde olsa bile (özellikle `fezz-runner` modeliyle), fonksiyon kütüphanesi birden çok kez reuse edilebilir. Ağır client'lar için yine `std::sync::OnceLock` gibi mekanizmaları kullanabilirsin, ancak unutmaman gereken nokta:

- Process crash ederse (panic, segfault), sonraki çağrı yeni bir process ile sıfırdan başlar.
- Bu nedenle state'i sadece performans için kullan, **doğruluk için değil**.

### Guidelines

1. **Blocking client kullan**: FFI interface senkron olduğu için HTTP/DB için bloklayan client'lar en sade yol.
2. **Panik yerine hata döndür**: Panik etmek yerine `FezzWireResponse` ile düzgün hata mesajları dön.
3. **Stateless tasarla**: İş mantığını her request bağımsız olacak şekilde yaz; global mutable state'e güvenme.
4. **Timeout'ları düşün**: Dış servis çağrılarına makul timeout'lar koy; child process askıda kalmasın.

## Demo

### 1. Build sample function and runner

```bash
cargo build --release -p example_todosapi -p fezz-runner -p hhrf
```

### 2. Fonksiyon kütüphanesini functions klasörüne koy

```bash
# todos@latest için fezz.json'daki isimle eşleşmeli
mkdir -p ./functions/todos@latest
cp target/release/libexample_todosapi.dylib ./functions/todos@latest/libexample_todosapi.dylib
```

`functions/todos@latest/fezz.json` örneği (repo'da zaten var, sadece referans için):

```json
{
  "id": "todos",
  "version": "latest",
  "entry": "libexample_todosapi.dylib",
  "routes": [
    {
      "path": "/hello",
      "method": "GET"
    }
  ]
}
```

### 3. (Opsiyonel) Runner'ı override et

Varsayılan olarak HHRF, `fezz-runner` binary'sini kullanır. Eğer kendi jailer'ını eklemek istiyorsan:

```bash
export FEZZ_RUNNER=fezz-runner   # veya kendi wrapper'ının path'i
```

### 4. HHRF server'ını çalıştır

```bash
export HHRF_ROOT=.
cargo run -p hhrf --release
```

### 5. Fonksiyonu test et

```bash
curl http://127.0.0.1:3000/rpc/todos@latest
```
