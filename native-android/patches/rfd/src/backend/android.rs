use crate::{
    backend::{
        AsyncFilePickerDialogImpl, AsyncFileSaveDialogImpl, AsyncFolderPickerDialogImpl,
        AsyncMessageDialogImpl, DialogFutureType, FilePickerDialogImpl, FileSaveDialogImpl,
        FolderPickerDialogImpl, MessageDialogImpl,
    },
    file_dialog::FileDialog,
    message_dialog::{MessageDialog, MessageDialogResult},
    FileHandle,
};
use std::path::PathBuf;

// Android file selection is owned by the host app's Storage Access Framework bridge.
// Until that bridge is installed, return cancellation rather than pretending rfd has a backend.
impl FilePickerDialogImpl for FileDialog {
    fn pick_file(self) -> Option<PathBuf> { None }
    fn pick_files(self) -> Option<Vec<PathBuf>> { None }
}

impl FileSaveDialogImpl for FileDialog {
    fn save_file(self) -> Option<PathBuf> { None }
}

impl FolderPickerDialogImpl for FileDialog {
    fn pick_folder(self) -> Option<PathBuf> { None }
    fn pick_folders(self) -> Option<Vec<PathBuf>> { None }
}

impl AsyncFilePickerDialogImpl for FileDialog {
    fn pick_file_async(self) -> DialogFutureType<Option<FileHandle>> { Box::pin(async { None }) }
    fn pick_files_async(self) -> DialogFutureType<Option<Vec<FileHandle>>> { Box::pin(async { None }) }
}

impl AsyncFolderPickerDialogImpl for FileDialog {
    fn pick_folder_async(self) -> DialogFutureType<Option<FileHandle>> { Box::pin(async { None }) }
    fn pick_folders_async(self) -> DialogFutureType<Option<Vec<FileHandle>>> { Box::pin(async { None }) }
}

impl AsyncFileSaveDialogImpl for FileDialog {
    fn save_file_async(self) -> DialogFutureType<Option<FileHandle>> { Box::pin(async { None }) }
}

impl MessageDialogImpl for MessageDialog {
    fn show(self) -> MessageDialogResult { MessageDialogResult::Cancel }
}

impl AsyncMessageDialogImpl for MessageDialog {
    fn show_async(self) -> DialogFutureType<MessageDialogResult> {
        Box::pin(async { MessageDialogResult::Cancel })
    }
}
