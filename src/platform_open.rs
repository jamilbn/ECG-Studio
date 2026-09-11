#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{ClassType, DeclaredClass, declare_class, msg_send_id, mutability};
    use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationDelegateReply};
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL,
    };

    static OPEN_FILE_QUEUE: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();

    pub struct OpenFileHandler {
        _delegate: Retained<AppDelegate>,
    }

    declare_class!(
        struct AppDelegate;

        unsafe impl ClassType for AppDelegate {
            type Super = NSObject;
            type Mutability = mutability::MainThreadOnly;
            const NAME: &'static str = "EcgStudioAppDelegate";
        }

        impl DeclaredClass for AppDelegate {
            type Ivars = ();
        }

        unsafe impl NSObjectProtocol for AppDelegate {}

        unsafe impl NSApplicationDelegate for AppDelegate {
            #[method(application:openURLs:)]
            fn application_open_urls(&self, _application: &NSApplication, urls: &NSArray<NSURL>) {
                let paths = (0..urls.count())
                    .filter_map(|index| unsafe { urls.objectAtIndex(index).path() })
                    .map(|path| PathBuf::from(path.to_string()))
                    .collect::<Vec<_>>();
                enqueue_paths(paths);
            }

            #[method(application:openFile:)]
            fn application_open_file(&self, _sender: &NSApplication, filename: &NSString) -> bool {
                enqueue_paths(vec![PathBuf::from(filename.to_string())]);
                true
            }

            #[method(application:openFiles:)]
            fn application_open_files(
                &self,
                sender: &NSApplication,
                filenames: &NSArray<NSString>,
            ) {
                let paths = (0..filenames.count())
                    .map(|index| PathBuf::from(unsafe { filenames.objectAtIndex(index) }.to_string()))
                    .collect::<Vec<_>>();
                enqueue_paths(paths);
                unsafe {
                    sender.replyToOpenOrPrint(NSApplicationDelegateReply::Success);
                }
            }
        }
    );

    impl AppDelegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = mtm.alloc().set_ivars(());
            unsafe { msg_send_id![super(this), init] }
        }
    }

    pub fn install_open_file_handler() -> OpenFileHandler {
        let mtm = MainThreadMarker::new().expect("macOS app delegate needs the main thread");
        let delegate = AppDelegate::new(mtm);
        let app = NSApplication::sharedApplication(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        OpenFileHandler {
            _delegate: delegate,
        }
    }

    pub fn take_open_file_paths() -> Vec<PathBuf> {
        let queue = OPEN_FILE_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        let Ok(mut queue) = queue.lock() else {
            return Vec::new();
        };
        queue.drain(..).collect()
    }

    fn enqueue_paths(paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let queue = OPEN_FILE_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut queue) = queue.lock() {
            queue.extend(paths);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::PathBuf;

    pub struct OpenFileHandler;

    pub fn install_open_file_handler() -> OpenFileHandler {
        OpenFileHandler
    }

    pub fn take_open_file_paths() -> Vec<PathBuf> {
        Vec::new()
    }
}

pub use imp::{install_open_file_handler, take_open_file_paths};
