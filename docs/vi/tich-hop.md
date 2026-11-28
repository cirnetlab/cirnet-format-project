# Tích hợp CIRNET Project Format Core

Interface dựng Project Explorer từ ID, loại, tiêu đề và quan hệ trong manifest, sau đó đọc tài liệu khi cần. Chuyển byte CircuitGraph gốc qua codec của Contracts và áp dụng xác thực theo ngữ cảnh của editor. Remote tiếp tục quản lý quyền lưu trữ máy chủ, lease và quyền chạy. CLI có thể dùng riêng các API inspect, verify và publish. Phiên bản WASM tương lai có thể dùng thư viện với `--no-default-features` và bổ sung adapter lưu trữ. Repo Project sở hữu vòng đời, revision và quyền trên EngineeringProject.

Khi lưu một tài liệu, Core đối chiếu digest của snapshot và gọi validator do ứng dụng cung cấp. Khi lưu nhiều tài liệu, Core ghi các đường dẫn mới rồi thay manifest sau cùng. Nếu tiến trình dừng trước khi thay manifest, manifest trước đó vẫn đọc được. Những file phát sinh chưa được tham chiếu có thể được kiểm tra và dọn riêng. Chỉ mục, file tạm và dữ liệu cá nhân nằm ngoài gói xuất theo quy tắc thành viên.

Khi nhập ZIP, Core xác minh nội dung ở vùng tạm do bên gọi kiểm soát, sau đó mới tạo thư mục đích và công bố manifest sau cùng. Đường dẫn phát hiện là symlink hoặc reparse point trong dự án sẽ bị từ chối. Đặt thư mục dự án và vùng tạm trong khu vực mà tiến trình không tin cậy không thể đổi đường dẫn đồng thời. Mức bền dữ liệu khi mất điện phụ thuộc hệ điều hành và hệ thống file. SHA-256 xác minh nội dung. Việc xác thực nguồn và cấp quyền chạy thuộc ứng dụng gọi.

Ví dụ kiểm tra ba phiên bản CircuitGraph dùng CLI của Contracts và giữ byte gốc. Xem hướng dẫn tiếng Anh để biết quy trình và giới hạn triển khai. [Hướng dẫn tiếng Anh](../integration.md) · [Đặc tả 1.0](../../spec/1.0/README.md)
