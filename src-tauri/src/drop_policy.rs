#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    NoCopy,
    Unsupported,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragDecision {
    Wry,
    Native,
    Rejected(RejectReason),
}
pub fn classify(
    existing_files: bool,
    has_image: bool,
    has_promise: bool,
    copy_allowed: bool,
) -> DragDecision {
    if !copy_allowed {
        DragDecision::Rejected(RejectReason::NoCopy)
    } else if existing_files {
        DragDecision::Wry
    } else if has_image || has_promise {
        DragDecision::Native
    } else {
        DragDecision::Rejected(RejectReason::Unsupported)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn files_precede_native_and_copy_is_required() {
        for file in [false, true] {
            for image in [false, true] {
                for promise in [false, true] {
                    assert_eq!(
                        classify(file, image, promise, false),
                        DragDecision::Rejected(RejectReason::NoCopy)
                    );
                    assert_eq!(
                        classify(file, image, promise, true),
                        if file {
                            DragDecision::Wry
                        } else if image || promise {
                            DragDecision::Native
                        } else {
                            DragDecision::Rejected(RejectReason::Unsupported)
                        }
                    );
                }
            }
        }
    }
}
