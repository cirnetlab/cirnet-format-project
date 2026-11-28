# Bảo mật gói

Gói di động dùng [profile ZIP32 1.0](../../../spec/1.0/README.md#portable-zip),
chứa manifest dự án, tài nguyên cục bộ đã đăng ký và một bảng kiểm kê
SHA-256. Tài nguyên kỹ thuật giữ cách mã hóa nguyên bản.

| Thao tác | Kết quả |
| --- | --- |
| Kiểm tra metadata | Manifest, tính toàn vẹn của manifest và thành viên được khai báo |
| Xác minh | Kiểm tra byte thành viên và vỏ tài liệu mà không công bố |
| Chuẩn bị bản nhập đã xác minh | Ghi byte thành viên đã xác minh vào vùng tạm do bên gọi kiểm soát |
| Công bố | Tạo đích mới từ dữ liệu đã chuẩn bị và công bố manifest sau cùng |
| Xuất | Ghi manifest và các tài nguyên dự án được chọn vào đích gói |

Kiểm tra sơ bộ đối chiếu bản ghi cục bộ và trung tâm, tên thành viên, chồng
lấn, kích thước và giới hạn nén trước khi đọc payload. Xác minh kiểm tra tập
thành viên chính xác, số byte và SHA-256 theo luồng. JSON và envelope hợp lệ
vẫn cần xác thực chuyên ngành để chỉnh sửa. Danh tính bên phát hành, quyền
công bố hoặc nhập, quyền thực thi và an toàn khi vận hành thiết bị là các
kiểm soát riêng. Mã nguồn, firmware, script và cấu hình đã nhập chỉ là dữ liệu.

Báo kết quả kiểm tra metadata với trạng thái tài nguyên chưa xác minh.
API `*_with_progress` gọi hàm nhận `PackageProgress` gồm giai đoạn,
tên thành viên nếu có và số byte của thành viên hiện tại. Trả về
`ControlFlow::Break(())` để hủy tại điểm kiểm tra. Xuất trực tiếp vào đích do
bên gọi cung cấp có thể để lại byte chưa hoàn chỉnh. Khi hủy xuất tệp
native, tệp đích vẫn chưa được công bố. Dùng vùng tạm nhập riêng có hạn
mức. Dùng `VerifiedPackage::publish_retryable` để giữ bản nhập đã chuẩn bị
sau khi bị từ chối trước công bố và đã dọn dẹp xong, kể cả khi hủy. Phương
thức chỉ dọn tệp đã xác minh do thao tác đặt vào đích và thư mục rỗng do
thao tác tạo. `PackagePublishError::Incomplete` báo đích được giữ lại khi
chưa chắc có thể dọn an toàn. Xác định quyền sở hữu trước khi xóa mục giữ
lại. Sau gián đoạn, dùng `inspect_package_destination` để kiểm tra dấu nhập
đang chờ hoặc manifest. Dấu dùng để chẩn đoán và không cấp quyền xóa đích
sau khi khởi động lại. Dành riêng đích cục bộ do bên gọi kiểm soát trong
suốt lúc công bố và hoàn tác. Manifest là điểm công bố. Kết quả đã công bố trả về
dự án đã mở cùng cảnh báo đồng bộ hoặc dọn dẹp nếu có.

## Đường dẫn di động và profile tương lai

Profile đường dẫn 1.0 dùng thành phần ASCII và loại bỏ thành phần ẩn, tên
thiết bị và đường dẫn trùng nhau khi bỏ qua hoa thường. Dùng đường dẫn an
toàn đã đăng ký cho thư mục làm việc và gói. Để thông tin xác thực, tệp
riêng tư và vị trí riêng của máy ngoài tập thành viên có thể trao đổi.
Tọa độ ngoài là khai báo. Chỉ bên gọi được cấp quyền mới phân giải hoặc
sao chép chúng.

Hỗ trợ Unicode cần profile tương thích tường minh về chuẩn hóa, cách viết
ổn định, quy tắc tương đương hoa thường, mã hóa tên trong kho lưu trữ và độ
dài đường dẫn trên các nền tảng. Cây mã nguồn cần dotfile phải có chính sách
theo phiên bản riêng, danh sách cho phép và kiểm tra bí mật cùng đường dẫn
riêng tư. Kiểm thử chuyển đổi, xung đột đường dẫn hiện có và trao đổi hai
chiều giữa các nền tảng trước khi bật từng khả năng. Vị trí riêng tư và bên
ngoài cần mô hình tin cậy và truy cập tường minh. Phiên bản 1.0 không tự động
phân giải các vị trí này.

<!-- source: docs/en/architecture/package-security.md -->
<!-- source-sha256: 0ffcce69285a854b3bd06ac54228f570f2ca722f09eeaf1ebc32450f682cc6a3 -->
