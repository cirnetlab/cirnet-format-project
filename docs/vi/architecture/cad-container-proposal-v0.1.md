# Đề xuất kiến trúc container CAD 0.1

Trạng thái **kiến trúc đề xuất**. Phiên bản tài liệu này xác định trách nhiệm
và yêu cầu tương thích cho container `.circad` tương lai.
[Profile Project Format 1.0](../../../spec/1.0/README.md) đã triển khai có thẩm
quyền đối với manifest, envelope và gói hiện hành. Có thể chuyển tệp
`.circad` như tài nguyên native opaque theo profile đó. Việc chỉnh sửa,
kiểm tra cấu trúc và chuyển đổi định dạng bên trong cần codec CAD tương lai.

## Quyền sở hữu lắp ráp và CAD

| Biểu diễn | Nội dung có thẩm quyền | Ranh giới sở hữu |
| --- | --- | --- |
| `.cir3d` | Bố trí mô hình được tham chiếu, instance có định danh ổn định, vị trí và góc xoay, phân cấp, ràng buộc bố trí, cấu hình lắp ráp và tham chiếu revision mô hình khi cần | Miền lắp ráp cho tổ hợp dự án |
| `.circad` đề xuất | Thiết kế tham số, bản phác, ràng buộc, dòng thời gian tính năng, hình học có thẩm quyền gồm B-rep native, thành phần và khớp CAD, tham số và biểu thức, cấu hình, vật liệu và diện mạo | `cirnet-cad` sở hữu ngữ nghĩa chuyên ngành, hình học, codec CAD và adapter SolidCraft tương lai |
| `.cirpkg` | Thành viên dự án có thể trao đổi, manifest và bảng kiểm kê toàn vẹn | Format Core sở hữu gói dự án bên ngoài |

Bản lắp ráp tham chiếu hình học CAD có thể chỉnh sửa theo định danh tài nguyên
và revision mô hình chính xác khi cần. Bên sở hữu CAD xác định thành phần
và khớp nội bộ CAD. Tổ hợp dự án dùng tham chiếu mô hình và dữ liệu bố trí
thay vì nhúng bản sao mô hình tham số.

## Phân công lưu trữ đề xuất

```text
Model.circad
  container.json       định danh và phiên bản container
  document.json        thiết kế CAD đã tạo và tham chiếu blob bắt buộc
  blobs/               payload native bắt buộc định địa chỉ bằng SHA-256
  cache/
    preview.json       metadata xem trước dẫn xuất tùy chọn
    preview.bin        byte hiển thị dẫn xuất tùy chọn
    model.json         metadata cache mô hình dẫn xuất tùy chọn
    bodies.bin         các khối đã tính toán dẫn xuất tùy chọn
```

Đây là bố cục minh họa. Bên sở hữu CAD chọn schema payload, yêu cầu cho từng
phần và encoding được kiểm chứng trong CAD Format Foundation. Phân biệt
metadata thiết kế và blob nguồn native bắt buộc với hình học đã tính toán
và bản xem trước tùy chọn. Blob nguồn cần cho việc dựng lại thiết kế thuộc
nội dung đã tạo, kể cả khi chứa hình học. Cache tùy chọn không hợp lệ, lỗi
thời hoặc thiếu được tính là cache miss sau khi kiểm tra sơ bộ container
và xác thực nội dung bắt buộc. Sự thiếu hoặc hỏng byte bắt buộc chặn mở để
chỉnh sửa. Lưới cache và bản xem trước chỉ phục vụ khung nhìn dẫn xuất.

Định danh blob bắt buộc bằng SHA-256 trên byte chính xác trước khi nén và
kiểm chứng cách nhận diện encoding. Dùng xác minh theo luồng có giới hạn
cho dữ liệu bắt buộc, đồng thời kiểm tra tính hợp lệ của cache bằng cả
định danh thiết kế nguồn và phiên bản cache của bên tạo. Digest kiểm tra
sự tương ứng và hỏng dữ liệu. Quyền và độ tin cậy cần bằng chứng riêng.

## Ranh giới phiên bản và codec

Tách phiên bản profile container CAD, schema miền CAD, từng encoding blob
và từng revision cache tùy chọn. Giữ chúng độc lập với Manifest, Document
Envelope và Package Profile 1.0 của Project Format.

Xác thực container đã biết riêng với schema CAD đã biết. Cung cấp các khả
năng riêng cho kiểm tra container, xác thực cấu trúc, xác thực chuyên ngành,
đọc, ghi, giữ nguyên byte và chuyển đổi. Có thể chuyển nguyên byte phiên bản
major chưa hỗ trợ khi tài nguyên ngoài được xử lý an toàn. Chỉ cho chỉnh sửa
khi bên sở hữu có thể xác thực thay đổi và giữ nội dung chưa biết. Chuyển
đổi do bên sở hữu đăng ký chỉ rõ phiên bản nguồn và đích. Bằng chứng tương
thích nên kiểm tra đầu vào thiết kế cũ, thành viên chưa biết, cache mất hiệu
lực và nội dung tương lai chưa hỗ trợ.

Bộ giải mã tài liệu đăng ký hiện tại của Format Core hỗ trợ envelope JSON và
JSON cũ đã biết. Chỉ thêm phần mở rộng tệp hoặc đăng ký codec không cung cấp
khả năng phân tích ZIP. Adapter biểu diễn trong tương lai cần đáp ứng những
yêu cầu sau cùng với [hợp đồng giao dịch](transactions.md).

- Nhận nguồn theo luồng có giới hạn hoặc hỗ trợ seek. Kiểm tra định danh
  container, đường dẫn và loại thành viên, số lượng, tùy chọn ZIP được hỗ trợ,
  kích thước và hạn mức giải nén trước khi nạp các phần. Giữ hạn mức gói
  ngoài độc lập với hạn mức bên trong bộ đọc CAD.
- Giữ băm byte chính xác tại ranh giới tài nguyên Format. Digest snapshot
  tạo tập đọc và hỗ trợ kiểm tra revision lạc quan. Chuẩn bị đầu ra do tác
  giả tạo bằng cùng chính sách giao dịch không ghi đè và công bố manifest
  cuối cùng, có xác thực codec tường minh của bên sở hữu. Phục hồi giữ bằng
  chứng chuẩn bị đã phân kỳ.
- Xác minh thành viên `.cirpkg` tại ranh giới gói bên ngoài. Không cần
  diễn giải định dạng bên trong khi chuyển nguyên byte một thành viên CAD
  opaque. Mở để chỉnh sửa xác minh riêng nội dung CAD bắt buộc và schema miền.
- Định tuyến khả năng tương tác `.sc`, STEP và GLB qua engine CAD. Giữ
  thẩm quyền tham số có thể chỉnh sửa trong tài liệu CAD khi xuất kết quả
  dẫn xuất hoặc nhập biểu diễn trao đổi.

## Tham chiếu SolidCraft và sử dụng lại

Revision nguồn đã rà soát `d94d83c2897df16607000e81e420b373bfa48ecc`.
Tài liệu tham chiếu cung cấp cách tách phiên bản container, tài liệu, blob,
cache và lưu trữ định địa chỉ theo nội dung. Định dạng `.sc` và định nghĩa
cache B-rep native của upstream vẫn là hợp đồng của upstream.

- [Container thiết kế](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/docs/design-container.md)
- [B-rep native](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/docs/native-brep.md)
- [Crate Format](https://github.com/tangency-labs/solidcraft/tree/d94d83c2897df16607000e81e420b373bfa48ecc/crates/format)
- [Lịch sử phiên bản tài liệu](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/crates/doc/src/format.rs)

Workspace upstream đã rà soát khai báo MIT OR Apache-2.0 cho mã nguồn. Bộ
nhận diện và phần hình biểu tượng được chỉ định có điều khoản riêng. Trước
khi sử dụng lại, rà soát đường dẫn chọn, tính tương thích giấy phép, thông
báo bản quyền và ghi công tài nguyên cùng bên sở hữu. Đề xuất dựa trên kiến
trúc đã công bố và không đưa mã nguồn hay hình ảnh upstream vào dự án.

- [Giấy phép MIT](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/LICENSE-MIT) · [Giấy phép Apache](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/LICENSE-APACHE)
- [Giấy phép thương hiệu](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/LICENSE-BRAND) · [Ghi công](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/ATTRIBUTION.md) · [Giấy phép bên thứ ba](https://github.com/tangency-labs/solidcraft/blob/d94d83c2897df16607000e81e420b373bfa48ecc/THIRD-PARTY-LICENSES.txt)

<!-- source: docs/en/architecture/cad-container-proposal-v0.1.md -->
<!-- source-sha256: 823ecb009fa15937bff46d38dd786d22a97fd521ee60748c08d3eb975104079b -->
