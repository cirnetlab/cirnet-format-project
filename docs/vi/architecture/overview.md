# Kiến trúc

CIRNET Project Format Core sở hữu cơ chế tuần tự hóa dự án có thể trao đổi.
Thư viện hỗ trợ Điện tử, Thiết kế PCB, Lắp ráp 3D, Hệ thống nhúng, FPGA,
Robot, IoT, Tính toán khoa học, phòng thí nghiệm ảo và phòng thí nghiệm vật lý
qua manifest, envelope tài liệu, quan hệ đã đăng ký và tài nguyên nguyên bản
được xử lý dưới dạng byte. Các bên sở hữu chuyên ngành định nghĩa ý nghĩa của
nội dung sơ đồ mạch, PCB, CAD, dữ liệu đo, thư viện và các nội dung khác.
Engine chuyên ngành có thể dùng định dạng qua codec do bên gọi cung cấp.

| Trách nhiệm | Phạm vi mã nguồn |
| --- | --- |
| Metadata dự án, bất biến của sổ đăng ký và codec manifest | `src/manifest/` |
| Envelope tài liệu và khả năng codec đã đăng ký | `src/document/` |
| Định danh tài nguyên, đường dẫn di động, tham chiếu và truy vấn phụ thuộc | `src/resource/` |
| JSON xác định và có giới hạn | `src/serialization/` |
| Nhận diện phiên bản, chuyển đổi tường minh và nhập CircuitGraph cũ | `src/migration/` |
| Digest và bảng kiểm kê gói | `src/integrity/` |
| Profile ZIP, kiểm tra sơ bộ, đọc, ghi và xác minh | `src/package/` |
| Lập kế hoạch, tập đọc, kiểm tra revision, công bố và phục hồi | `src/transaction/` |
| Dự án đang làm việc và thao tác hệ thống tệp | `src/storage/native/` |
| Điểm vào API, lỗi và giới hạn đầu vào | `src/lib.rs`, `src/error.rs`, `src/limits.rs` |

Thuật toán di động giữa các nền tảng nhận byte hoặc trait I/O chuẩn.
Lớp lưu trữ native cung cấp thao tác hệ thống tệp và điều phối việc ghi dự án.
Tài liệu API Rust công khai lấy từ mã nguồn crate.
[Đặc tả định dạng theo phiên bản](../../../spec/1.0/README.md) định nghĩa
ngữ nghĩa của dữ liệu lưu trữ.

[Project Manager](https://github.com/cirnetlab/cirnet-project-manager) sở hữu
EngineeringProject và vòng đời tài liệu, tổ chức logic, revision vòng đời
và tham chiếu artifact. Repo này dùng Format Core qua API Rust công khai.
Revision SHA-256 của Format là token nội dung byte dùng để kiểm tra xung đột
theo cơ chế lạc quan. Contracts sở hữu schema trao đổi dùng chung và SDK sinh
từ schema. Interface sở hữu phần hiển thị của trình chỉnh sửa. Lưu trữ và cấp
quyền máy chủ vẫn thuộc Remote trong triển khai Lab hiện hành. Hardware thực
thi bảo vệ tại trạm. Xem [bảng phân quyền sở hữu của CIRNETLAB](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).

[Đề xuất kiến trúc container CAD](cad-container-proposal-v0.1.md) tách bố trí
lắp ráp .cir3d khỏi dữ liệu mô hình CAD có thể chỉnh sửa và mô tả ranh giới
biểu diễn dự kiến.

<!-- source: docs/en/architecture/overview.md -->
<!-- source-sha256: dae70f356145f8f9bc3c2c6f55c2bc57d7cb25e4c98c73d82a0da957367dc75a -->
