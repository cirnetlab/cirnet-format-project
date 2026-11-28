# Mô hình dự án

Định dạng lai mở dự án từ một manifest `.cirproj` UTF-8 JSON ở thư mục gốc.
Trình chỉnh sửa có thể tạo cây tổng quan từ sổ đăng ký và chỉ đọc tài nguyên
cần cho thao tác hiện tại.

| Phần mở rộng | Biểu diễn | Mục đích |
| --- | --- | --- |
| `.cirproj` | UTF-8 JSON | Manifest dự án kỹ thuật |
| `.cirsch` | Envelope UTF-8 JSON | Sơ đồ mạch |
| `.cirpcb` | Envelope UTF-8 JSON | PCB |
| `.cir3d` | Envelope UTF-8 JSON và tham chiếu mô hình | Lắp ráp 3D |
| `.cirlib` | Envelope UTF-8 JSON và tham chiếu tài nguyên | Thư viện kỹ thuật |
| `.cirpkg` | ZIP32, JSON và tài nguyên nguyên bản | Gói dự án có thể trao đổi |

Giữ định dạng nguyên bản của mã nguồn, firmware, mô hình, dữ liệu thiết bị
đo và kết quả khoa học. Sổ đăng ký ghi định danh và vị trí của chúng.
Envelope JSON chứa metadata tài liệu, nội dung chuyên ngành có cấu trúc và
dữ liệu mở rộng. JSON chuẩn hóa giúp dễ đọc diff. Giữ byte nguồn chính xác
đối với nội dung cũ hoặc chưa được hỗ trợ mà Core chỉ chuyển tiếp.

## Định danh và tham chiếu tài nguyên

ID dự án và ID tài nguyên ổn định khi đổi đường dẫn. ID tài liệu, tài nguyên
và thư viện ngoài dùng chung một không gian tên. Truy vấn đồ thị tài nguyên
lập chỉ mục theo ID và đường dẫn, phân giải tham chiếu xuôi và ngược, và tìm
toàn bộ phụ thuộc bắc cầu. Tạo đồ thị bằng `Manifest::resource_graph` và dùng
`ResourceGraph::impact` để xem tham chiếu đi vào cùng các bên sở hữu liên
quan bắc cầu trước khi bỏ hoặc di chuyển tài nguyên. Di chuyển đổi vị trí
trong khi tham chiếu vẫn dùng cùng ID.

Cạnh phụ thuộc và thư viện giữa các tài liệu phải phi chu trình. Tham chiếu
thông thường, liên kết cấu hình và kết quả có thể tạo chu trình. Đồ thị xử lý
các quan hệ cấp dự án. Tham chiếu trong payload, kết nối chân linh kiện và
phụ thuộc riêng của tài nguyên theo schema của bên sở hữu. Tệp kỹ thuật
nguyên bản vẫn là thành viên cục bộ của sổ đăng ký. Tọa độ thư viện ngoài là
metadata cho đến khi bên gọi có quyền phân giải nó.

## Khả năng xử lý tài liệu

Kiểm tra envelope và giải mã cấu trúc xác lập thông tin ở cấp vỏ tài liệu.
Dùng `CodecRegistry` để xác định khả năng chuyên ngành theo loại tài liệu,
phiên bản envelope và phiên bản schema. Đăng ký tường minh khả năng xác thực,
ghi và chuyển đổi của bên sở hữu bằng `CodecRegistration`. Có thể kiểm tra
và chuyển nguyên byte tài liệu chưa được hỗ trợ khi thao tác cho phép bảo
toàn dữ liệu. Định danh trong sổ đăng ký và dữ liệu chưa hiểu vẫn được giữ
khi không thể chỉnh sửa.

Xem [đặc tả 1.0](../../../spec/1.0/README.md) để biết giới hạn về trường,
giá trị số, quan hệ và đường dẫn.

<!-- source: docs/en/architecture/project-model.md -->
<!-- source-sha256: 1e22c116338e3b83c481492cf701c3a603a70710b870eba23d57c903c8466b6f -->
