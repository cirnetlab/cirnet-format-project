# Tích hợp ứng dụng

| Bên dùng | Phạm vi tích hợp |
| --- | --- |
| Interface và Studio | Dựng Project Explorer từ ID, loại, tiêu đề và liên kết trong sổ đăng ký. Chỉ tải tài liệu khi có yêu cầu. Hỏi bên sở hữu chuyên ngành về khả năng chỉnh sửa schema hỗ trợ. |
| Remote | Cấp quyền lưu trữ và truyền dữ liệu trước khi xử lý định dạng. Cung cấp vùng tạm riêng có hạn mức để chuẩn bị bản nhập. Lease máy chủ, điều khiển Lab và phê duyệt thao tác vật lý vẫn thuộc các bên sở hữu tương ứng. |
| CLI | Cung cấp riêng thao tác kiểm tra metadata, xác minh và công bố. Hiển thị kết quả kiểm tra metadata cùng trạng thái tài nguyên chưa xác minh. Yêu cầu chọn đích mới tường minh để công bố. |
| Desktop | Giới hạn quyền truy cập native theo đường dẫn dự án, gói và vùng tạm đã chọn. Hiển thị xung đột revision, chẩn đoán phục hồi và quyền cho bên gọi. |
| WASM | Bắt đầu với `--no-default-features` cho API byte hoặc đánh giá feature package cho đầu vào trong bộ nhớ hỗ trợ seek. Kiểm chứng đích đã chọn và triển khai lưu trữ trình duyệt ở bên tiêu thụ. Dùng giải mã không mất dữ liệu cho payload cần chính xác. |

Mở snapshot đang làm việc bằng `Project::open`. Dùng ID trong sổ đăng ký làm
khóa lựa chọn ổn định. Lấy revision của mỗi tài nguyên bị tác động hoặc được
dùng khi đọc nội dung của nó. Tạo `ChangeSet` cho lần ghi native và dùng
`Project::apply` với sổ đăng ký codec tường minh cho thao tác sửa nội dung tài
liệu. Hiển thị kết quả đọc và kiểm tra cấu trúc riêng với khả năng chỉnh sửa.
Xem [Giao dịch](../architecture/transactions.md).

Với `.cirpkg`, `package::inspect` đọc metadata và `package::verify_only` xác
minh kho lưu trữ `Read + Seek` mà không truy cập đích. Dùng
`package::export_from` với `PackageSource` và đích `Write + Seek` để xuất
qua các nền tảng. `package::verify` native chuẩn bị bản nhập trong vùng tạm.
Công bố kết quả đã xác minh vào thư mục mới do bên gọi kiểm soát.
Xem [Bảo mật gói](../architecture/package-security.md).

Contracts sở hữu các profile CircuitGraph dùng chung. Chuyển byte gốc cho
trình xác thực của bên sở hữu và tuân theo quy tắc chỉnh sửa của họ. Project
sở hữu revision vòng đời, cộng tác và quyền truy cập EngineeringProject.
Revision SHA-256 của Core chỉ mô tả nội dung tệp cục bộ. Cấp quyền Lab-First
thuộc Remote, còn bảo vệ tại trạm thuộc Hardware.
Xem [bảng phân quyền sở hữu](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).

<!-- source: docs/en/guides/integration.md -->
<!-- source-sha256: 13c06193f69855863a606dd32cc07fdc4bf194878b43ebf30c610f50f75b742a -->
