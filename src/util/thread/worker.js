// One thread of a module built with shared memory. The thread that started
// this worker posts the glue's URL, the compiled module, the shared memory
// and the address of the closure to run.
self.onmessage = async (event) => {
  const [glueUrl, module, memory, job] = event.data;
  let glue, instance;
  try {
    glue = await import(glueUrl);
    instance = await glue.default({ module_or_path: module, memory });
  } catch (e) {
    // The closure never ran; the thread that started this one drops it.
    self.postMessage(String(e?.stack ?? e));
    self.close();
    return;
  }
  glue.__divvun_runtime_thread_start(job);
  instance.__wbindgen_thread_destroy?.();
  self.close();
};
