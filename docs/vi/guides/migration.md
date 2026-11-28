# Chuyển đổi

## Chuyển ứng dụng từ 0.1.0 sang 0.2.0

Manifest, envelope tài liệu và gói ZIP hiện có được hỗ trợ giữ profile 1.0.
Crate 0.2.0 quản lý phiên bản từng profile độc lập với nhau, với schema chuyên
ngành và với Contracts dùng chung. Chỉ cập nhật phụ thuộc Rust không yêu cầu
ghi lại dự án được hỗ trợ.

1. Chọn các feature Cargo cần dùng rồi build lại ứng dụng.
2. Chuyển thay đổi tài nguyên từ `Project::commit` sang `Project::apply`.
   Cung cấp `ChangeSet` với revision manifest dự kiến cùng digest của tài
   nguyên bị tác động và phụ thuộc đọc đã khai báo. Đăng ký codec chuyên
   ngành xác thực nội dung được chỉnh sửa. Chỉ dùng điểm vào
   `Project::commit` được giữ lại cho thay đổi metadata.
3. Tiếp tục dùng `Project::save_document` khi chỉ lưu một tài liệu và đầu
   vào revision cùng trình xác thực đáp ứng thao tác. Dùng change set khi
   thao tác có phụ thuộc cứng hoặc dựa vào tệp cục bộ khác.
4. Khi có xung đột revision, tải lại trạng thái bị tác động và áp dụng lại
   chỉnh sửa của người dùng. Xử lý phục hồi cục bộ còn chờ trước khi đổi
   cách triển khai hoặc di chuyển thư mục làm việc.

Dùng mục đích phù hợp với thao tác tệp. Có thể di chuyển tài liệu chưa được
hỗ trợ bằng byte gốc. Viết envelope hoặc schema khác cần đường chuyển đổi
tường minh được hỗ trợ. Với chuyển đổi native, đăng ký codec có khả năng
chuyển đổi cho cả nguồn và đích. Dùng bộ nhận diện phiên bản định dạng trước
khi chọn bước chuyển đổi. Bước chuyển đổi do bên sở hữu cung cấp phải tạo
đúng đích đã khai báo, và đích phải qua các kiểm tra áp dụng. Giữ byte nguồn và xác
minh kết quả trước khi thay trạng thái dự án.

## Nhập CircuitGraph cũ

[Ví dụ adapter Contracts](../../../examples/contracts_compat.rs) nhập
CircuitGraph 1.0, 1.1 và 1.2. Cung cấp đường dẫn tới CLI Contracts đáng tin
cậy và thư mục gốc nguồn tương thích.

```sh
cargo run --locked --example contracts_compat -- <trusted-cli> <contracts-source-root>
```

Core truyền chính xác byte đầu vào tới profile cấu trúc
`AnySupportedCircuitGraph` của Contracts. Byte gốc được giữ nguyên trong
tệp đồ thị đã đăng ký. Duy trì ID tài nguyên manifest riêng với
`circuit_id` chuyên ngành. Bên sở hữu trình chỉnh sửa hoặc thực thi phải
xác thực theo ngữ cảnh. Với phiên bản chưa hỗ trợ, giữ đầu vào gốc và
chọn bước chuyển đổi của bên sở hữu khi có.

<!-- source: docs/en/guides/migration.md -->
<!-- source-sha256: ae32b43398483f15003eee2b0458fbf6acf8d963ecf4ea5ef8812474fc7c7b7d -->
