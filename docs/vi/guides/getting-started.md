# Bắt đầu

Khi tắt feature mặc định, crate cung cấp API di động giữa các nền tảng cho
manifest, tài liệu, chuyển đổi, tài nguyên và tính toàn vẹn. Bật `package`
để xử lý ZIP qua các nền tảng, hoặc dùng feature mặc định `native` để làm
việc với dự án trên hệ thống tệp và nhập xuất gói.

## Đọc và ghi byte manifest

```rust
use cirnet_format_project::{Limits, Manifest, decode_manifest};

let limits = Limits::default();
let manifest = Manifest::new("project-01", "Bench supply", vec!["electronics".into()]);
let bytes = manifest.to_bytes(&limits)?;
let loaded = decode_manifest(&bytes, &limits)?;
assert_eq!(loaded.project_id, "project-01");
# Ok::<(), cirnet_format_project::Error>(())
```

## Tạo dự án đang làm việc

Từ thư mục gốc kho mã, chọn thư mục đầu ra mới rồi chạy
[ví dụ dự án](../../../examples/working_project.rs).

```sh
cargo run --locked --example working_project -- ./my-lab
```

Ví dụ tạo dự án native, ghi envelope chứa nội dung qua trình xác thực
đã đăng ký cho schema mẫu, rồi xuất `.cirpkg`. Dùng `Project::open` với
đường dẫn `.cirproj` đã chọn cho dự án hiện có. Thao tác mở tải manifest.
`read_document` tải tệp được yêu cầu và trả về byte cùng revision cần cho
lần ghi sau.

Tạo tài liệu tham chiếu crate bằng `cargo doc --locked --all-features --no-deps`.
Xem [Tích hợp](integration.md) về phạm vi của ứng dụng và dịch vụ.

<!-- source: docs/en/guides/getting-started.md -->
<!-- source-sha256: 187d7ec43345c92ecdbdcc72160adf579d324cfc34bb9cadbaa63d63d24b0b5b -->
