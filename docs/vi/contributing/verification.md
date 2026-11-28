# Kiểm chứng

Chạy các phép kiểm tra từ thư mục gốc kho mã. Ghi lại revision hoặc artifact
nguồn, đích và toolchain, lệnh chính xác, log và SHA-256 của artifact được chia
sẻ. Dùng `PASS`, `FAIL`, `NOT_RUN` và `BLOCKED` cho kết quả quan sát hoặc
khoảng trống kiểm chứng.

## Kiểm tra ứng viên hiện tại

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features package --all-targets -- -D warnings
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo test --locked --no-default-features --features package
cargo build --locked --release --all-features
cargo build --locked --release --no-default-features
cargo build --locked --release --no-default-features --features package
cargo doc --locked --all-features --no-deps
```

- Chạy ma trận native trên Windows, Linux và macOS theo
  [workflow kiểm chứng](../../../.github/workflows/verify.yml). Trên Windows,
  chạy `cargo test --locked --test native symlinked_native_resources_are_rejected -- --ignored`
  ở nơi fixture có thể tạo junction.
- Lặp lại các cấu hình build và kiểm thử cần thiết với trình biên dịch trong
  `rust-version` của `Cargo.toml`. Phiên bản trình biên dịch khai báo cần
  bằng chứng riêng. Sau khi cài đích WASM, chạy
  `cargo check --locked --no-default-features --target wasm32-unknown-unknown`
  và kiểm chứng riêng feature package di động khi được hỗ trợ.
- Xác thực manifest, envelope và bảng kiểm kê được tạo bằng trình kiểm tra
  Draft 2020-12 rồi so sánh với hành vi runtime nghiêm ngặt. Kiểm tra
  [ví dụ tương thích Contracts](../../../examples/contracts_compat.rs)
  bằng CLI và revision nguồn đáng tin của bên sở hữu. Ghi lại định danh của cả hai.
- Kiểm tra revision cũ giữa các thao tác, xác thực bị từ chối, các giai đoạn
  phục hồi, xung đột đường dẫn, byte tài liệu chưa được hỗ trợ, JSON lỗi và
  giới hạn kho lưu trữ. Khi fuzz parser hoặc kho lưu trữ, ghi lại công cụ
  kiểm thử thuộc tính hoặc fuzz, corpus, đích, thời lượng và kết quả.

Chạy bộ vector Draft 2020-12 chung bằng module Ajv 8.20.0 được phê duyệt
và đối chiếu kết quả Rust bằng `cargo test --locked --test schema_conformance`.
`cargo test --locked --test parser_mutations` chạy các ca phân tích cú pháp
và thuộc tính có giới hạn, tái lập được. Chạy kiểm chứng ứng dụng mẫu với
thư mục đầu ra mới.

```sh
node tests/check-schemas.cjs <path-to-ajv-dist-2020.js>
cargo run --locked --release --example qualified_consumer -- <new-directory>
```

## Kiểm tra tài liệu

Từ thư mục gốc kho mã này, dùng `check-docs --product --root .` từ checkout
CIRNET Rules được phê duyệt. Truyền `--new-file <path>` cho mỗi trang Markdown
được thêm. Chạy `git diff --check` và xác nhận diff đúng phạm vi tài liệu.

Rà soát liên kết tương đối và anchor tiêu đề, các cặp đường dẫn Anh–Việt,
ý nghĩa và hash nguồn của bản dịch, cùng thông tin nguồn gốc trong changelog.
Xem quy ước trong [hướng dẫn viết tài liệu](writing-documentation.md).
Chạy kiểm tra build có mục tiêu khi đường dẫn đã chỉnh sửa được dùng để
biên dịch, đóng gói, kiểm thử hoặc chạy CI.

## Tái lập bằng chứng hiệu năng

Dùng `cargo bench --locked --bench workloads` và ghi lại kích thước tải,
số mẫu, chế độ build, hệ thống tệp và trạng thái cache. So sánh cùng thao tác
với cùng đầu vào, profile, máy và nơi ghi đầu ra. Báo phân bố độ trễ, thông
lượng đối với tải byte và phương pháp đo đỉnh bộ nhớ. Kiểm tra metadata,
chỉ xác minh, xác minh kèm chuẩn bị, và công bố hoàn chỉnh đo các công việc
khác nhau. Ghi riêng chi phí đồng bộ hoặc sao chép khi đo được.

## Bằng chứng lịch sử

[Báo cáo kiểm chứng và benchmark 0.1.0 gốc](../../history/v0.1.0-verification.md)
ghi kết quả Foundation cục bộ trên Windows, nguồn gốc baseline và checksum
artifact. [Lần chạy CI của baseline](https://github.com/cirnetlab/cirnet-format-project/actions/runs/46088261341)
áp dụng cho [commit `cc3a257`](https://github.com/cirnetlab/cirnet-format-project/commit/cc3a257e03560943b531341d1520bf7f0892eb2c).
[Báo cáo kiểm chứng hardening 0.2.0](../../history/v0.2.0-hardening-qualification.md)
ghi lại ứng viên đã kiểm tra trên Windows, các phạm vi còn thiếu kiểm chứng,
so sánh benchmark và digest artifact.

<!-- source: docs/en/contributing/verification.md -->
<!-- source-sha256: d531508021657c4ec88652d1d5852704fd436259783775bfc16b49f215bb0314 -->
