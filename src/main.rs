use std::{io, ptr};
use std::io::Write;
use std::net::TcpStream;
use std::os::fd::AsRawFd;

fn main() {
    // 记录等待事件的数量
    let mut event_counter = 0;

    // 创建一个 event queue
    let queue = unsafe { ffi::kqueue() };
    // handle errors, just panicking
    if queue < 0 {
        panic!("{}", io::Error::last_os_error());
    }

    // 创建保存 stream 的队列
    let mut streams = vec![];

    // 创建 5 个延迟响应的请求链接
    for i in 1..6 {
        let addr = "127.0.0.1:9527";
        let mut stream = TcpStream::connect(addr).unwrap();

        let delay = (5-i) * 1000;
        let request = format!(
            "GET /delay/{}/url/http://delay.com HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: close\r\n\
             \r\n",
            delay,
        );
        stream.write_all(request.as_bytes()).unwrap();
        stream.set_nonblocking(true).unwrap();

        // 注册该socket上的 Read事件通知
        // Kevent 用来指定我们想要注册的事件以及使用标志的其他配置的地方
        // `EVFILT_READ` 表示这是一个 `Read` 兴趣
        // `EV_ADD` 表示我们正在向队列中添加一个新事件
        // `EV_ENABLE` 意味着我们希望事件在触发时返回
        // `EV_ONESHOT` 表示我们希望通风口在第一次出现时从队列中删除。
        // 如果我们不这样做，我们需要在完成套接字后手动“注销”我们的兴趣
        // （这很好，但对于这个例子来说，第一次删除它会更容易）
        // 你可以在这里阅读更多关于标志和选项的信息：
        // https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kevent.2.html
        let event = ffi::Kevent{
            ident: stream.as_raw_fd() as u64,
            filter: ffi::EVFILT_READ,
            flags: ffi::EV_ADD | ffi::EV_ENABLE | ffi::EV_ONESHOT,
            fflags: 0,
            data: 0,
            udata: i,
        };

        let changelist = [event];

        // 这是我们实际向队列注册兴趣的调用。
        // 根据传入的参数，对 kevent 的调用行为不同。
        // 传入空指针作为超时指定无限超时
        let res = unsafe {ffi::kevent(
            queue, // kqueue 句柄
            changelist.as_ptr(), //
            1, // changelist 列表的长度
            ptr::null_mut(), // 事件队列 非必要
            0, // 事件列表的长度 非必要
            ptr::null(), // 超时 非必要 null pointer 超时时间无限长
        )};

        if res < 0 {
            panic!("{}", io::Error::last_os_error());
        }

        // 让 `stream` 超出 Rust 的作用域会自动运行其析构函数以关闭套接字。
        // 我们通过坚持直到我们完成来防止这种情况
        streams.push(stream);
        event_counter += 1;
    }

    // 等event事件通知完
    while event_counter > 0 {
        // API 希望我们传入一个 `Kevent` 结构数组。
        // 这就是操作系统将发生的事情反馈给我们的方式。
       let mut events: Vec<ffi::Kevent> = Vec::with_capacity(10);

        // 这个调用实际上会阻塞，直到事件发生。 传入空指针作为超时无限期等待
        // 现在操作系统挂起我们的线程进行上下文切换并处理其他事情 - 或者只是保留电源。
        let res = unsafe {
            ffi::kevent(
                queue,
                ptr::null(), // 阻塞等待响应的时候 changelist 为空
                0,    // changelist 长度相应也为空
                events.as_mut_ptr(), // we expect to get events back
                events.capacity() as i32, // 希望接收事件的个数
                ptr::null(), // 超时时间
            )
        };

        // 此结果将返回发生的事件数（如果有）或负数（如果是错误）
        if res < 0 {
            panic!("{}", io::Error::last_os_error());
        }

        unsafe { events.set_len(res as usize)};

        println!("start receiver");
        for event in events {
            println!("RECEIVED: {}", event.udata);
            event_counter -= 1;
        }
    }

    let res = unsafe { ffi::close(queue) };
    if res < 0 {
        panic!("{}", io::Error::last_os_error());
    }
    println!("FINISHED");
}

mod ffi {
    pub const EVFILT_READ: i16 = -1;
    pub const EV_ADD: u16 = 0x1;
    pub const EV_ENABLE: u16 = 0x4;
    pub const EV_ONESHOT: u16 = 0x10;

    #[link(name = "c")]
    extern "C" {
        /// Returns: positive: file descriptor, negative: error
        pub(super) fn kqueue() -> i32;
        pub(super) fn kevent(
            kq: i32,
            changelist: *const Kevent,
            nchanges: i32,
            eventlist: *mut Kevent,
            nevents: i32,
            timeout: *const Timespec,
        ) -> i32;

        pub fn close(d: i32) -> i32;
    }

    #[derive(Debug)]
    #[repr(C)]
    pub(super) struct Timespec {
        /// seconds
        tv_sec: isize,
        /// Nanoseconds
        v_nsec: usize,
    }

    impl Timespec {
        pub fn from_millis(milliseconds: i32) -> Self {
            let seconds = milliseconds / 1000;
            let nanoseconds = (milliseconds % 1000) * 1000 * 1000;
            Timespec {
                tv_sec: seconds as isize,
                v_nsec: nanoseconds as usize,
            }
        }
    }

    #[derive(Debug, Clone, Default)]
    #[repr(C)]
    pub struct Kevent {
        pub ident: u64,
        pub filter: i16,
        pub flags: u16,
        pub fflags: u32,
        pub data: i64,
        pub udata: u64,
    }
}
