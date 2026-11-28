# Viết và duy trì tài liệu Project Format

[English](../../en/contributing/writing-documentation.md) · [Tài liệu](../index.md)

Tuân theo [hướng dẫn của kho mã](../../../AGENTS.md) và
[chính sách tài liệu](https://github.com/cirnetlab/cirnet-rules/blob/master/en/policies/documentation.md)
của CIRNET Rules. Tìm chủ đề và liên kết đến các trang hiện có trước khi thêm
trang. Đặt mỗi quy trình hoặc phần giải thích trong tài liệu có trách nhiệm
duy trì nội dung đó rồi dẫn liên kết từ các mục lục.

## Chọn vị trí lưu tài liệu

| Vị trí | Mục đích |
| --- | --- |
| [README](../../../README.md) ở gốc | Giới thiệu crate và dẫn đến mục lục của cả hai ngôn ngữ. |
| `docs/en/` và `docs/vi/` | Duy trì tài liệu kiến trúc, hướng dẫn người dùng và quy trình đóng góp hiện hành ở các đường dẫn tương ứng. |
| [Đặc tả và schema theo phiên bản](../../../spec/1.0/README.md) | Xác định định dạng lưu trữ. Tuân theo chính sách phiên bản và schema khi thay đổi ngữ nghĩa. |
| [Mục lục changelog](../../changelogs/README.md) | Duy trì tệp `CHANGELOG_V<major>.md` dùng chung bằng tiếng Anh, mỗi phiên bản major có một tệp khi cần. |
| [Bằng chứng lịch sử](../../history/v0.1.0-verification.md) | Giữ ngôn ngữ gốc, revision nguồn, kết quả, digest artifact và phạm vi. |

Core sở hữu việc tuần tự hóa và đóng gói dự án. Dẫn CIRNET Rules cho kiến
trúc cấp tổ chức và bên sở hữu chuyên ngành cho ngữ nghĩa kỹ thuật của họ.
Lưu kế hoạch và tiến độ riêng của từng tác vụ trong
[Task Continuity](https://github.com/cirnetlab/cirnet-rules/blob/master/en/docs/distribution.md#task-continuity).

Trước khi di chuyển hoặc hợp nhất trang, kiểm tra các liên kết đến trang và
tham chiếu trong mã nguồn, kiểm thử, cấu hình build và CI. Giữ lại các lệnh,
yêu cầu, giới hạn và thông tin nguồn gốc riêng. Dùng đường dẫn tương đối cho
liên kết nội bộ và liên kết cố định theo commit đầy đủ cho bằng chứng lịch
sử chính xác từ kho mã khác.

## Cập nhật cặp tài liệu tiếng Anh và tiếng Việt

Dùng tên tệp viết thường và phân cách bằng dấu gạch nối cho hướng dẫn hiện
hành, một tiêu đề H1, tiêu đề mô tả rõ nội dung và khai báo ngôn ngữ cho khối
mã. Viết bản tiếng Anh chuẩn trong `docs/en/` và dịch sang cùng đường dẫn
tương đối trong `docs/vi/`. Giữ nguyên đường dẫn, lệnh, định danh API, phiên
bản và trạng thái kiểm chứng. Rà soát ý nghĩa kỹ thuật trước khi cập nhật
metadata cuối bản dịch. Dùng đường dẫn tương đối từ gốc kho mã đến bản tiếng
Anh cho `source` và SHA-256 viết thường của đúng nội dung UTF-8/LF tiếng Anh cho
`source-sha256`.

## Ghi thay đổi và kiểm chứng

Ghi thay đổi đang chờ vào `Unreleased`. Liệt kê các mốc có phiên bản SemVer
đầy đủ theo thứ tự mới nhất trước. Dùng các mục `Added`, `Changed`, `Fixed`,
`Deprecated`, `Removed` và `Security` phù hợp, bỏ các mục trống. Dẫn manifest
phiên bản và revision nguồn. Chỉ ghi ngày công bố khi đã xác minh và ghi đúng
trạng thái của các ứng viên phát triển.

Rà soát cả hai ngôn ngữ, đường dẫn và anchor nội bộ rồi chạy
[các phép kiểm tra tài liệu](verification.md#kiểm-tra-tài-liệu).

<!-- source: docs/en/contributing/writing-documentation.md -->
<!-- source-sha256: cc34899d05f4ec020b5c3c02e07fab388cefd491290fc72f93d7c61282c65cec -->
