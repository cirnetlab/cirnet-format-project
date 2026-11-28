# Phát triển

Chạy lệnh Cargo từ thư mục gốc kho mã. Dùng `rust-version` trong `Cargo.toml`
cho trình biên dịch tối thiểu và toolchain stable hiện hành cho ma trận chính.
Lockfile ghim bộ phụ thuộc của ứng viên. Đặt đầu ra build, dự án tạm và
bằng chứng benchmark vào đường dẫn được cấp.

```sh
cargo fmt --all -- --check
cargo test --locked --all-features
cargo test --locked --no-default-features
cargo doc --locked --all-features --no-deps
```

Duy trì một crate chính. Dùng [bản đồ trách nhiệm mã nguồn](../architecture/overview.md)
khi di chuyển logic hoặc công bố API. Giữ kiểm thử hồi quy và fixture tương
thích. Thêm kiểm thử tập trung cho ranh giới tin cậy, thao tác lưu trữ hoặc
trường hợp lỗi thay đổi. Ghi lại mốc cơ sở và phép so sánh tương đương khi
thay đổi hành vi ảnh hưởng hiệu năng.

[Đặc tả theo phiên bản](../../../spec/1.0/README.md) sở hữu ngữ nghĩa định dạng
lưu trữ. Duy trì schema và fixture tuân thủ cùng thay đổi runtime tương thích.
Cập nhật đường dẫn tương ứng trong `docs/en/` và `docs/vi/` cùng thay đổi.
Rà soát nghĩa tiếng Việt trước khi cập nhật digest nguồn tiếng Anh. Giữ lịch
sử phát hành bằng tiếng Anh trong [changelog dùng chung](../../changelogs/README.md).

Dùng [Kiểm chứng](verification.md) để xem đầy đủ quy trình đánh giá và benchmark.

<!-- source: docs/en/contributing/development.md -->
<!-- source-sha256: 9e421dd783cf102e2a0e8339bc49b253c9ea38c459d7f7918205894e4da9035e -->
