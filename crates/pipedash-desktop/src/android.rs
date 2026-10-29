use std::path::Path;
use std::sync::{
    Mutex,
    PoisonError,
};

use jni::{
    errors::{
        Error,
        ThrowRuntimeExAndDefault,
    },
    objects::{
        JClass,
        JObject,
        JString,
    },
    Env,
    EnvUnowned,
    JavaVM,
};

static HEADLESS: Mutex<bool> = Mutex::new(false);

pub fn init_tls() -> Result<(), Error> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread(|env| {
        let context = unsafe { JObject::from_raw(env, ctx.context().cast()) };
        rustls_platform_verifier::android::init_with_env(env, context)
    })
}

fn init_headless(env: &mut Env, context: &JObject) -> Result<(), Error> {
    let mut ready = HEADLESS.lock().unwrap_or_else(PoisonError::into_inner);
    if *ready {
        return Ok(());
    }
    let vm = env.get_java_vm()?;
    let global = env.new_global_ref(context)?.into_raw();
    let local = env.new_local_ref(context)?;
    rustls_platform_verifier::android::init_with_env(env, local)?;
    unsafe { ndk_context::initialize_android_context(vm.get_raw().cast(), global.cast()) };
    *ready = true;
    let _ = rustls::crypto::ring::default_provider().install_default();
    if let Err(e) = crate::credential::init() {
        tracing::warn!("Failed to initialize the credential store: {}", e);
    }
    Ok(())
}

#[jni::jni_mangle("com.henrique.pipedash.plugin.RefreshBridge")]
pub fn run_refresh<'local>(
    mut env: EnvUnowned<'local>, _class: JClass<'local>, context: JObject<'local>,
    data_dir: JString<'local>,
) -> JString<'local> {
    env.with_env(|env| {
        init_headless(env, &context)?;
        let data_dir = data_dir.try_to_string(env)?;
        let notices = crate::local_notifications::refresh_headless(Path::new(&data_dir));
        JString::new(
            env,
            serde_json::to_string(&notices).unwrap_or_else(|_| "[]".to_owned()),
        )
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[jni::jni_mangle("com.henrique.pipedash.plugin.RefreshBridge")]
pub fn cancel_refresh<'local>(_env: EnvUnowned<'local>, _class: JClass<'local>) {
    crate::local_notifications::cancel_headless();
}
