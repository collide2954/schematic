(() => {
  const root = document.getElementById("play");
  if (!root || !window.Worker || !window.WebAssembly) return;

  const EXAMPLES = {
    tables: `(define sales
  (table {:city ["Oslo" "Rome" "Bern" "Oslo"]
          :year [2020 2021 2021 2021]
          :units [12 7 nil 30]}))

(aggregate sales :city {:total [sum :units] :orders [count]})

(order-by (where sales (> (:units sales) 10)) :units :desc)`,
    arrays: `(define prices #[10 20 30 nil])

(* prices 1.25)
(mean prices)
(where prices (> prices 15))
(string-upcase #["oslo" nil "rome"])`,
    dates: `(define d #2024-03-15)

(date-format d "EEEE, d MMMM yyyy")
(date-trunc d :month)
(- #2024-04-01 d)
(date-range #2024-01-30 #2024-02-02)
(format "{} sold {:,} units ({:.1%})" "Oslo" 42000 0.256)`,
    patterns: `(define (shape-name shape)
  (match shape
    ([:circle r] (format "circle, radius {}" r))
    ([:rect w h] :when (= w h) (format "square of side {}" w))
    ([:rect w h] (format "{} by {} rectangle" w h))
    (else "something else")))

(map shape-name [[:circle 2] [:rect 3 3] [:rect 2 5] 42])

(let (([first & more] [1 2 3]))
  [first more])`,
    charts: `(define sales
  (table {:city ["Oslo" "Rome" "Bern" "Oslo"]
          :year [2020 2021 2021 2021]
          :units [12 7 nil 30]}))

(-> (bars sales :city :units {:by :year})
    (hline 20 "goal")
    (title "Units by city"))

(histogram #[3 5 4 8 9 7 12 11 14 13] {:bins 4})`,
  };

  const code = root.querySelector(".play-code");
  const out = root.querySelector(".play-out");
  const status = root.querySelector(".play-status");
  const runButton = root.querySelector(".play-run");
  const resetButton = root.querySelector(".play-reset");
  const picker = root.querySelector(".play-example");

  let worker = null;
  let pending = null;
  let nextId = 1;
  let touched = false;

  function startWorker() {
    worker = new Worker("play-worker.js");
    worker.onmessage = (event) => {
      if (!pending || event.data.id !== pending.id) return;
      clearTimeout(pending.timer);
      const { resolve } = pending;
      pending = null;
      resolve(event.data.text);
    };
  }

  function ask(action, source) {
    if (!worker) startWorker();
    return new Promise((resolve) => {
      const id = nextId++;
      const timer = setTimeout(() => {
        if (!pending || pending.id !== id) return;
        pending = null;
        worker.terminate();
        worker = null;
        resolve("Stopped after five seconds. The interpreter was restarted, so definitions are gone.\n");
      }, 5000);
      pending = { id, resolve, timer };
      worker.postMessage({ id, action, source });
    });
  }

  function show(text) {
    out.textContent = "";
    if (!text) {
      out.textContent = "(no output)";
      return;
    }
    for (const line of text.replace(/\n$/, "").split("\n")) {
      const span = document.createElement("span");
      span.textContent = line + "\n";
      if (line.startsWith("error:")) span.className = "err";
      out.appendChild(span);
    }
  }

  let running = false;
  async function run() {
    if (running) return;
    running = true;
    status.textContent = "Running";
    runButton.disabled = true;
    const text = await ask("eval", code.value);
    show(text);
    status.textContent = "";
    runButton.disabled = false;
    running = false;
  }

  async function reset() {
    await ask("reset");
    status.textContent = "Fresh interpreter";
    setTimeout(() => { if (status.textContent === "Fresh interpreter") status.textContent = ""; }, 1500);
  }

  function load(name) {
    code.value = EXAMPLES[name];
    touched = false;
    run();
  }

  runButton.addEventListener("click", run);
  resetButton.addEventListener("click", reset);
  picker.addEventListener("change", () => load(picker.value));

  code.addEventListener("input", () => { touched = true; });
  code.addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      run();
    } else if (event.key === "Tab") {
      event.preventDefault();
      const { selectionStart, selectionEnd, value } = code;
      code.value = value.slice(0, selectionStart) + "  " + value.slice(selectionEnd);
      code.selectionStart = code.selectionEnd = selectionStart + 2;
    }
  });

  const warm = () => {
    startWorker();
    status.textContent = "Loading the interpreter";
    ask("eval", "nil").then(() => {
      status.textContent = "";
      if (!touched) run();
    });
  };
  if ("requestIdleCallback" in window) {
    requestIdleCallback(warm, { timeout: 2500 });
  } else {
    setTimeout(warm, 800);
  }
})();
