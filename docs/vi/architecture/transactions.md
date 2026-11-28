# Giao dịch và phục hồi

Dự án native đang làm việc có digest revision của manifest. Đọc chính xác
byte tài liệu bằng `Project::read_document` và dùng revision trả về từ lần
đọc đó. Dùng `Project::resource_revision` cho các tài nguyên cục bộ khác.
Tạo `ChangeSet` với digest manifest dự kiến rồi truyền digest tệp bị tác động
và phụ thuộc đọc đã khai báo vào `ChangeSet::expect`. Lấy token của dữ liệu
đã dùng để thực hiện chỉnh sửa. Bao gồm các tài nguyên cục bộ hiện có
trong tập phụ thuộc cứng bắc cầu của mỗi tài nguyên bị tác động. Khi chỉ
lưu tài liệu, digest manifest có thể giữ nguyên. Kiểm tra revision tài
nguyên phát hiện trường hợp dùng snapshot cũ.

Dùng `Project::read_set` cho một tập tài nguyên cục bộ có giới hạn do bên
gọi chọn. Phương thức trả về snapshot tài liệu và digest tài nguyên. Kiểm
tra lại bằng `Project::validate_read_set` hoặc gắn vào changeset bằng
`ChangeSet::include_read_set` để kiểm tra khi công bố. Bao gồm mọi tài nguyên
được dùng để chỉnh sửa. Bộ lập kế hoạch dùng chung phép duyệt phụ thuộc
cứng với đồ thị tài nguyên. Thư viện điều phối các bên ghi tuân theo khóa.
Tiến trình bên ngoài vẫn có thể thay đổi hệ thống tệp giữa các lần kiểm tra.

| Thao tác | Mục đích |
| --- | --- |
| Tạo | Đăng ký tài nguyên mới và đặt byte vào đường dẫn chưa sử dụng |
| Thay nội dung | Sửa tài nguyên hiện có sau khi đối chiếu digest dự kiến |
| Di chuyển | Đưa nguyên byte đến vị trí mới cho cùng ID tài nguyên |
| Chuyển đổi | Áp dụng chuyển đổi được hỗ trợ một cách tường minh và xác thực kết quả |
| Bỏ tài nguyên | Chủ động bỏ đăng ký sau khi kiểm tra ảnh hưởng. Giữ tệp cũ để dọn dẹp tường minh. |

Việc đọc có thể diễn ra khi cần. Khối lượng công việc của giao dịch phụ thuộc
vào tài nguyên bị tác động và các lượt đọc đã khai báo. Xác thực cấu trúc áp
dụng cho metadata lưu trữ và envelope tài liệu. Cần xác thực do bên sở hữu
chuyên ngành cung cấp khi sửa nội dung tài liệu. Giữ nguyên byte tài liệu chưa
được hỗ trợ khi di chuyển và giữ nội dung chưa hiểu khi chỉnh sửa tài liệu
được hỗ trợ.

## API ghi

Dùng `Project::apply_with_outcome` cho changeset tường minh. Kết quả chứa
trạng thái công bố, revision manifest hiện tại, ID tài nguyên bị tác động,
digest kết quả khi có, trạng thái độ bền và phục hồi. Kết quả `Err` từ chối
thao tác trước điểm công bố. Kết quả đã công bố có thể kèm lỗi hoàn tất,
vì vậy cần đọc trạng thái trước khi thử lại. Trên hệ thống thiếu thao tác
đồng bộ thư mục, kết quả báo `DurabilityStatus::PlatformLimited`. Lỗi trước
công bố có thể để lại tệp đã chuẩn bị. Dùng `Project::inspect_recovery` để
kiểm tra trước khi ghi tiếp. `Project::apply` duy trì kết quả tương thích. Dùng `Project::save_document_with_outcome` để ghi một tài
liệu khi validator và revision truyền vào đủ cho thao tác.

Dùng `Project::commit` cho metadata cấp dự án. Sửa metadata của tài nguyên
đã đăng ký cần revision từ snapshot dùng khi chỉnh sửa. Truyền tập đọc cho
`Project::commit_metadata` hoặc dùng changeset tường minh.

## Công bố và phục hồi

Với giao dịch thay đổi manifest, chuẩn bị các tệp mới ở đường dẫn chưa dùng
và công bố manifest mới sau cùng. Manifest cũ vẫn trỏ được tới tài nguyên cũ
cho đến thời điểm đó. Giao dịch lưu một tài liệu thay thế nguyên tử một tệp
theo cùng chính sách revision và xác thực. Thư viện đồng bộ tệp tạm
trong thư mục chứa tài liệu trước khi thay thế đích. Chỉ giao dịch thay đổi
manifest cần journal. Nếu bị gián đoạn khi lưu một tệp, đích vẫn ở revision
cũ hoặc mới và có thể còn tệp tạm chưa dùng trong thư mục đó.

| Kết quả | Việc bên gọi cần làm |
| --- | --- |
| Bị từ chối trước công bố | Giữ trạng thái hiện có và xử lý xung đột, lỗi xác thực hoặc lỗi I/O |
| Đã commit | Đọc từ snapshot đã commit |
| Đã công bố và gặp `Error::Durability` | Mở lại và kiểm tra đường dẫn được báo trước khi thử lại |
| Đã công bố và gặp `Error::PublishedCleanup` | Giữ kết quả đã công bố và phục hồi tường minh vùng chuẩn bị cục bộ |
| Bị gián đoạn | Kiểm tra bản ghi phục hồi cục bộ và yêu cầu phục hồi tường minh |

`Project::inspect_recovery` kiểm tra journal có giới hạn trong thư mục riêng
`.cirnet-transaction`. `Project::recover` dọn vùng chuẩn bị riêng sau khi
giai đoạn chuẩn bị bị gián đoạn hoặc khi điểm công bố khớp với journal.
Phương thức báo các tệp đã đặt nhưng chưa được tham chiếu để bên gọi xem xét
và giữ lại tệp khi chưa xác định quyền sở hữu. Nếu điểm công bố đã lệch,
phương thức giữ journal và các tệp để bên gọi xem xét. Xác định quyền sở
hữu trên hệ thống tệp trước khi xóa đường dẫn được báo.

Dùng khóa dự án để điều phối các bên ghi qua thư viện. Dùng thư mục làm việc
cục bộ đáng tin cậy và giữ quyền kiểm soát các thành phần đường dẫn. Lớp
native từ chối symlink và reparse point Windows khi phát hiện. Máy chủ cần
cách ly mạnh hơn đối với tiến trình có thể đồng thời thay thành phần đường
dẫn đã kiểm tra.

Đồng bộ tệp và thư mục có giới hạn tùy nền tảng và hệ thống tệp. Cần kiểm
chứng riêng độ bền khi mất điện và hệ thống tệp dùng chung hoặc qua mạng.
Mở lại manifest và các tệp bị tác động sau lỗi có kết quả chưa rõ trước khi
ghi tiếp.

## Revision liên kết dự án và giới hạn tập đọc

Thay đổi liên kết cứng của dự án cần token đã lấy cho tập phụ thuộc hoặc
thư viện cục bộ bị ảnh hưởng, gồm tài nguyên đích hiện có. Liên kết tham
chiếu mềm, cấu hình và kết quả là thay đổi metadata trừ khi bên gọi khai
báo phụ thuộc nội dung. Cạnh cứng giữ nguyên không buộc thao tác ghi metadata
khác phải băm cả dự án. Dùng changeset có revision khi đổi quan hệ cứng
cấp dự án.

Dùng `Project::read_revisions` để tính digest byte theo luồng mà không giữ
tài liệu hoặc cấp quyền nội dung chuyên ngành. Phương thức điều phối các
tiến trình ghi tuân theo khóa khi băm tài nguyên đã chọn. Khi giữ nội dung
tài liệu, `read_set_with_buffer_limit` giới hạn tổng byte tài liệu giữ trong
bộ nhớ độc lập với giới hạn từng tài nguyên và giới hạn tổng hiện có.
Cả hai chế độ tạo token để giao dịch kiểm tra lại trước khi công bố.

`Project::apply_report` và `save_document_report` cung cấp lỗi từ chối
có kiểu cùng yêu cầu phục hồi. Kiểm tra trạng thái phục hồi đang chờ hoặc
chưa rõ trước khi thử lại. `Project::create_with_outcome` báo dự án đã đặt
cùng trạng thái công bố tệp ngay cả khi chưa chắc độ bền của thư mục.
Đây là API bổ sung. Các điểm vào cũ giữ kiểu kết quả.

<!-- source: docs/en/architecture/transactions.md -->
<!-- source-sha256: 0052005f5561923997ea19d0a0a7c24b8b3f22ee4707935fb89ee8005d3a5189 -->
