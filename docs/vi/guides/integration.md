# Tích hợp ứng dụng

| Bên dùng | Phạm vi tích hợp |
| --- | --- |
| Project Manager | Dùng crate này cho manifest và sổ đăng ký trao đổi, tập đọc đã xác thực, giao dịch, codec và gói. Project Manager sở hữu ngữ nghĩa vòng đời và cây logic. |
| Interface và Studio | Dựng Project Explorer từ ID, loại, tiêu đề và liên kết trong sổ đăng ký. Chỉ tải tài liệu khi có yêu cầu. Hỏi bên sở hữu chuyên ngành về khả năng chỉnh sửa schema hỗ trợ. |
| Remote | Cấp quyền lưu trữ và truyền dữ liệu trước khi xử lý định dạng. Cung cấp vùng tạm riêng có hạn mức để chuẩn bị bản nhập. Lease máy chủ, điều khiển Lab và phê duyệt thao tác vật lý vẫn thuộc các bên sở hữu tương ứng. |
| CLI | Cung cấp riêng thao tác kiểm tra metadata, xác minh và công bố. Hiển thị kết quả kiểm tra metadata cùng trạng thái tài nguyên chưa xác minh. Yêu cầu chọn đích mới tường minh để công bố. |
| Desktop | Giới hạn quyền truy cập native theo đường dẫn dự án, gói và vùng tạm đã chọn. Hiển thị xung đột revision, chẩn đoán phục hồi và quyền cho bên gọi. |
| WASM | Bắt đầu với `--no-default-features` cho API byte hoặc đánh giá feature package cho đầu vào trong bộ nhớ hỗ trợ seek. Kiểm chứng đích đã chọn và triển khai lưu trữ trình duyệt ở bên tiêu thụ. Dùng giải mã không mất dữ liệu cho payload cần chính xác. |

Mở snapshot đang làm việc bằng `Project::open`. Dùng ID trong sổ đăng ký làm
khóa lựa chọn ổn định. Lấy tài liệu và revision tài nguyên cục bộ dùng
cho lần chỉnh sửa bằng `Project::read_set`. Gắn token vào `ChangeSet` rồi
dùng `Project::apply_with_outcome` cùng sổ đăng ký codec của bên sở hữu.
Đọc lại khi có xung đột và kiểm tra kết quả đã công bố trước khi thử lại
sau cảnh báo độ bền hoặc dọn dẹp. Hiển thị kiểm tra cấu trúc riêng với
khả năng chỉnh sửa. Xem [Giao dịch](../architecture/transactions.md).

Với `.cirpkg`, `package::inspect` đọc metadata và `package::verify_only` xác
minh kho lưu trữ `Read + Seek` mà không truy cập đích. Dùng
`package::export_from` với `PackageSource` và đích `Write + Seek` để xuất
qua các nền tảng. `package::verify` native chuẩn bị bản nhập trong vùng tạm.
Công bố kết quả đã xác minh vào thư mục mới do bên gọi kiểm soát bằng
`publish_retryable`. Xem [Bảo mật gói](../architecture/package-security.md).

[Ứng dụng Rust độc lập](../../../examples/qualified_consumer.rs) chạy thử
các API này với schema chuyên ngành dành riêng cho kiểm thử. Ví dụ kiểm
chứng quy trình thư viện cục bộ. Tích hợp sản phẩm, lưu trữ trình duyệt và
cấp quyền thực thi cần bằng chứng riêng.

Contracts sở hữu các profile CircuitGraph dùng chung. Chuyển byte gốc cho
trình xác thực của bên sở hữu và tuân theo quy tắc chỉnh sửa của họ.
[Project Manager](https://github.com/cirnetlab/cirnet-project-manager) sở hữu
revision vòng đời EngineeringProject và tham chiếu artifact.
Revision SHA-256 của Core mô tả nội dung tệp cục bộ. Revision máy chủ và
cấp quyền Lab-First thuộc bên sở hữu dịch vụ hiện hành là Remote.
Hardware duy trì bảo vệ tại trạm.
Xem [bảng phân quyền sở hữu](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).

<!-- source: docs/en/guides/integration.md -->
<!-- source-sha256: b391e5adb0f4bd40c1dc4311b86b1122e1eaba672ac36219a625b7dd0eb71f76 -->
