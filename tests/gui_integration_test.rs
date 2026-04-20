use std::rc::Rc;
use std::cell::RefCell;
use VoiceInput::gui::VoiceInputGui;
use VoiceInput::state::AppState;

fn init_test_backend() {
    i_slint_backend_testing::init_no_event_loop();
}

#[test]
fn test_gui_creation_and_state_display() {
    init_test_backend();
    let gui = VoiceInputGui::new().unwrap();

    gui.update_state(&AppState::Idle);
    assert_eq!(gui.app().get_current_state(), "idle");

    gui.update_state(&AppState::Recording);
    assert_eq!(gui.app().get_current_state(), "recording");

    gui.update_state(&AppState::Transcribing);
    assert_eq!(gui.app().get_current_state(), "transcribing");
}

#[test]
fn test_gui_result_text() {
    init_test_backend();
    let gui = VoiceInputGui::new().unwrap();

    gui.set_result_text("你好世界");
    assert_eq!(gui.app().get_result_text(), "你好世界");
}

#[test]
fn test_gui_error_message() {
    init_test_backend();
    let gui = VoiceInputGui::new().unwrap();

    gui.set_error_message("网络超时");
    assert_eq!(gui.app().get_error_message(), "网络超时");
}

#[test]
fn test_gui_recording_duration() {
    init_test_backend();
    let gui = VoiceInputGui::new().unwrap();

    gui.set_recording_duration(42);
    assert_eq!(gui.app().get_recording_seconds(), 42);
}

#[test]
fn test_gui_callbacks_fire() {
    init_test_backend();
    let gui = VoiceInputGui::new().unwrap();

    let start_fired = Rc::new(RefCell::new(false));
    let finish_fired = Rc::new(RefCell::new(false));
    let cancel_fired = Rc::new(RefCell::new(false));

    gui.on_start_recording({
        let start_fired = start_fired.clone();
        move || { *start_fired.borrow_mut() = true; }
    });
    gui.on_finish_recording({
        let finish_fired = finish_fired.clone();
        move || { *finish_fired.borrow_mut() = true; }
    });
    gui.on_cancel({
        let cancel_fired = cancel_fired.clone();
        move || { *cancel_fired.borrow_mut() = true; }
    });

    gui.app().invoke_start_recording();
    assert!(*start_fired.borrow());

    gui.app().invoke_finish_recording();
    assert!(*finish_fired.borrow());

    gui.app().invoke_cancel();
    assert!(*cancel_fired.borrow());
}
