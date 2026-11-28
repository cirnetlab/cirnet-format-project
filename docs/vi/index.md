# CIRNET Project Format Core

[English](../en/index.md) · [README kho mã](../../README.md)

Dùng thư viện Rust để kiểm tra dự án kỹ thuật đang làm việc, lưu tài nguyên
hoặc trao đổi gói dự án. Crate Rust, các profile định dạng lưu trữ và schema
chuyên ngành có số phiên bản độc lập.

## Sử dụng và tích hợp

- [Bắt đầu](guides/getting-started.md) giới thiệu API native và API đa nền tảng.
- [Tích hợp](guides/integration.md) hướng dẫn kết nối với ứng dụng và công cụ.
- [Chuyển đổi](guides/migration.md) hướng dẫn cập nhật ứng dụng dùng 0.1.0 và nhập CircuitGraph cũ.

## Kiến trúc và định dạng

- [Tổng quan kiến trúc](architecture/overview.md) giải thích trách nhiệm của các module và bên sở hữu.
- [Mô hình dự án](architecture/project-model.md) mô tả tệp, định danh và tham chiếu.
- [Giao dịch và phục hồi](architecture/transactions.md) mô tả thao tác ghi native.
- [Bảo mật gói](architecture/package-security.md) mô tả giới hạn của kho lưu trữ và đường dẫn.
- [Đặc tả và schema định dạng 1.0](../../spec/1.0/README.md) xác định các định dạng lưu trữ.

- [Đề xuất container CAD 0.1](architecture/cad-container-proposal-v0.1.md) xác định ranh giới biểu diễn tương lai.

## Đóng góp và kiểm chứng

- [Phát triển](contributing/development.md) hướng dẫn quy trình làm việc với kho mã.
- [Kiểm chứng](contributing/verification.md) hướng dẫn kiểm tra kho mã và ghi nhận bằng chứng.
- [Viết tài liệu](contributing/writing-documentation.md) hướng dẫn vị trí tệp, dịch thuật và rà soát.

## Lịch sử và cấp phép

- [Lịch sử thay đổi theo phiên bản major](../changelogs/README.md) ghi lại các thay đổi bằng tiếng Anh.
- [Báo cáo kiểm chứng và benchmark 0.1.0](../history/v0.1.0-verification.md) lưu bằng chứng Foundation gốc bằng tiếng Anh.
- [Kiểm chứng hardening 0.2.0](../history/v0.2.0-hardening-qualification.md) ghi các kiểm tra và so sánh Windows trước đây.
- [Phạm vi cấp phép](../../LICENSING.md) mô tả các điều khoản áp dụng.

<!-- source: docs/en/index.md -->
<!-- source-sha256: f9fe68b236beb660946504d17dd44e884bc3306d4651bc4017baf086f1d80672 -->
