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
| Kiểm tra revision, chuẩn bị thay đổi và phục hồi | `src/transaction/` |
| Dự án đang làm việc và thao tác hệ thống tệp | `src/storage/native/` |
| Điểm vào API, lỗi và giới hạn đầu vào | `src/lib.rs`, `src/error.rs`, `src/limits.rs` |

Thuật toán di động giữa các nền tảng nhận byte hoặc trait I/O chuẩn.
Lớp lưu trữ native cung cấp thao tác hệ thống tệp và điều phối việc ghi dự án.
Tài liệu API Rust công khai lấy từ mã nguồn crate.
[Đặc tả định dạng theo phiên bản](../../../spec/1.0/README.md) định nghĩa
ngữ nghĩa của dữ liệu lưu trữ.

Contracts sở hữu schema trao đổi dùng chung và SDK sinh từ schema. Project sở
hữu vòng đời EngineeringProject, ACL và ID revision dịch vụ. Revision SHA-256
cục bộ là token nội dung dùng để kiểm tra xung đột theo cơ chế lạc quan.
Interface có thể đưa dữ liệu đang làm việc vào trình chỉnh sửa. Trong luồng
Lab-First, Remote cấp quyền thao tác và quản lý lease, còn Hardware thực thi
bảo vệ tại trạm. Xem [bảng phân quyền sở hữu của CIRNETLAB](https://github.com/cirnetlab/cirnet-rules/blob/master/en/references/architecture-routing.md).

<!-- source: docs/en/architecture/overview.md -->
<!-- source-sha256: 6fcfe98f7029f05c139ab832ea42fd36b15f8f35eb39e3159b6c5bd3d983423c -->
