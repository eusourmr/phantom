//! Regression corpus for ordered classic script discovery in Phantom 2L-5.

use phantom_engine::{Engine, PageScriptTask};

#[test]
fn classic_scripts_are_captured_in_document_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new();
    engine.load_html("<script>var one = 1;</script><script src='/two.js'></script><script>var three = 3;</script><script type='module'>ignored()</script><script async src='/async.js'></script>")?;
    assert_eq!(engine.page_script_tasks(), vec![
        PageScriptTask::Inline("var one = 1;".to_owned()),
        PageScriptTask::External("/two.js".to_owned()),
        PageScriptTask::Inline("var three = 3;".to_owned()),
    ]);
    Ok(())
}
