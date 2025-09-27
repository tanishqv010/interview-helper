import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { invoke } from "@tauri-apps/api/core";
import { register, unregisterAll } from "@tauri-apps/plugin-global-shortcut";
import { useEffect, useState, useRef } from "react";
import ReactMarkdown from "react-markdown";
import "./App.css";

const promptTemplates: Record<string, string> = {
  "single-correct-mcq": `From the provided screenshot(s), create concise single-correct multiple-choice question(s):
- Derive questions from core concepts, definitions, formulas, or edge cases in the image(s)
- Each question must have exactly ONE correct option; the number of options is NOT fixed. Choose an appropriate number of options based on the content (e.g., 3–6)
- Make distractors plausible but clearly wrong to a prepared learner
- Output format:
  Q: <question>
  A) <option>
  B) <option>
  C) <option>
  ... (continue as needed)
  Correct: <letter>
- Keep wording unambiguous and concise; use markdown for any math/code
`,
  "multiple-correct-mcq": `From the provided screenshot(s), create concise multiple-correct multiple-choice question(s):
- Derive questions from core concepts, procedures, and edge cases in the image(s)
- Each question can have MORE THAN ONE correct option; the number of options is NOT fixed. Choose an appropriate number of options based on the content (e.g., 4–7)
- Distractors should be plausible yet incorrect; avoid trick options
- Output format:
  Q: <question>
  A) <option>
  B) <option>
  C) <option>
  ... (continue as needed)
  Correct: <comma-separated letters>  (e.g., A,C)
- Keep wording unambiguous and concise; use markdown for any math/code
`,
  "code-without-comments": `Generate a complete and optimized solution to the problem described in the screenshot(s).
- Infer the full problem statement, constraints, and edge cases from the screenshot(s)
- Output exactly one code block containing fully working code
- Do not include comments or extra prose
- Include all necessary headers and fast I/O when helpful
- Also state Big-O time and space complexity (Best, Average, Worst) in a single line labeled 'Complexity:' before the code block
`,
  "code-with-explanation": `Generate a complete and optimized solution to the problem described in the screenshot(s).
- First, summarize the problem in 2-3 sentences
- Then provide the algorithm and explicit Big-O time and space complexity (Best, Average, Worst) in concise bullet points
- Finally, output exactly one code block with fully working code (include headers)
`,
  "beast-mode": `BEAST MODE ACTIVATED! 🚀

Extract ALL content from the provided images and analyze them comprehensively:

For MCQ Questions:
- Extract the complete question text
- Extract ALL options (A, B, C, D, etc.)
- Identify if it's single-correct or multiple-correct
- Extract any additional context, diagrams, or formulas
- Provide the correct answer(s)

For Coding Questions:
- Extract the complete problem statement
- Extract all constraints and requirements
- Extract input format specifications
- Extract output format specifications
- Extract all test cases and examples
- Extract any function signatures or driver code provided
- Identify the programming language if specified

For Mixed Content:
- Extract everything systematically
- Categorize each type of content
- Provide comprehensive analysis

Output format:
## Content Analysis
[Detailed extraction of all text, formulas, diagrams, etc.]

## Question Type
[MCQ/Coding/Mixed]

## Extracted Content
[Complete structured extraction]

## Ready for Advanced AI Processing
[All content formatted for final AI processing]
`,
};

export default function App() {
  const [prompt, setPrompt] = useState(promptTemplates["code-without-comments"]);
  const [output, setOutput] = useState("");
  const [loading, setLoading] = useState(false);
  // Let the window size follow content; start undefined and avoid forcing size
  // Removed window size state; not needed after simplifying layout
  const [opacity, setOpacity] = useState(0.8);
  const [queueLength, setQueueLength] = useState(0);
  const [model, setModel] = useState("gemini-2.5-pro");
  const [outputFormat, setOutputFormat] = useState("code-without-comments");
  const [language, setLanguage] = useState("C++");
  // Removed unused resize refs
  const lastAddTime = useRef(0);

  // async function handleFullScreenCaptureAndSend() {
  //   setLoading(true);
  //   setOutput("");
  //   try {
  //     const path = await invoke<string>("capture_full_screen");
  //     const result = await invoke<string>("call_gemini_with_image", {
  //       prompt,
  //       imagePath: path,
  //     });
  //     setOutput(result);
  //   } catch (err) {
  //     setOutput("Error: " + err);
  //   } finally {
  //     setLoading(false);
  //   }
  // }

  async function handleAddImageToQueue() {
    // Debounce: only allow one capture per 500ms
    const now = Date.now();
    if (now - lastAddTime.current < 500) {
      return;
    }
    lastAddTime.current = now;

    try {
      const length = await invoke<number>("add_image_to_queue");
      setQueueLength(length);
    } catch (err) {
      console.error("Error adding image to queue:", err);
    }
  }

  async function handleSendAllImages() {
    setLoading(true);
    setOutput("");
    try {
      if (outputFormat === "beast-mode") {
        // Use BEAST MODE processing
        let finalPrompt = language ? `${prompt}\n\nUse the programming language: ${language}.` : prompt;
        if (language === "C++") {
          finalPrompt += "\n\nAdditional requirements for C++:\n- Do NOT use any fast I/O boilerplate (e.g., ios::sync_with_stdio(false), cin.tie(nullptr)).\n- Include 'using namespace std;'.";
        }
        const result = await invoke<string>("call_beast_mode", {
          prompt: finalPrompt,
        });
        setOutput(result);
      } else {
        // Use regular processing
        let finalPrompt = language ? `${prompt}\n\nUse the programming language: ${language}.` : prompt;
        if (language === "C++") {
          finalPrompt += "\n\nAdditional requirements for C++:\n- Do NOT use any fast I/O boilerplate (e.g., ios::sync_with_stdio(false), cin.tie(nullptr)).\n- Include 'using namespace std;'.";
        }
        const result = await invoke<string>("call_gemini_with_image_queue", {
          prompt: finalPrompt,
        });
        setOutput(result);
      }
    } catch (err) {
      setOutput("Error: " + err);
    } finally {
      setLoading(false);
    }
  }

  async function handleClearQueue() {
    try {
      await invoke("clear_queue");
      setQueueLength(0);
      setOutput("");
      setLoading(false); // Stop any ongoing processing
    } catch (err) {
      console.error("Error clearing queue:", err);
    }
  }

  async function updateQueueLength() {
    try {
      const length = await invoke<number>("get_queue_length");
      setQueueLength(length);
    } catch (err) {
      console.error("Error getting queue length:", err);
    }
  }

  useEffect(() => {
    async function setupShortcuts() {
      try {
        // Arrow key nudges
        await register("CommandOrControl+Shift+Up", () => {
          invoke("nudge_window", { direction: "up", step: 50 });
        });
        await register("CommandOrControl+Shift+Down", () => {
          invoke("nudge_window", { direction: "down", step: 50 });
        });
        await register("CommandOrControl+Shift+Left", () => {
          invoke("nudge_window", { direction: "left", step: 50 });
        });
        await register("CommandOrControl+Shift+Right", () => {
          invoke("nudge_window", { direction: "right", step: 50 });
        });
        await register("CommandOrControl+Shift+Enter", handleSendAllImages);
        await register("CommandOrControl+Shift+H", () => {
          handleAddImageToQueue();
        });
        await register("CommandOrControl+Shift+R", () => {
          handleClearQueue();
        });
        await register("CommandOrControl+Shift+]", () => {
          setOpacity((prev) => Math.min(0.9, Math.round((prev + 0.05) * 20) / 20));
        });
        await register("CommandOrControl+Shift+[", () => {
          setOpacity((prev) => Math.max(0.1, Math.round((prev - 0.05) * 20) / 20));
        });
        await register("CommandOrControl+Shift+B", async () => {
          try {
            const visible = await invoke<boolean>("toggle_window_visibility");
            setOpacity(visible ? 0.9 : 0.1);
          } catch (e) {
            console.error("Toggle visibility failed", e);
          }
        });
        await register("CommandOrControl+Shift+Q", () => {
          invoke("quit_app");
        });

        // Initialize queue length
        updateQueueLength();

        console.log("Shortcuts registered");
      } catch (e) {
        console.error("Shortcut registration failed", e);
      }
    }

    setupShortcuts();
    return () => {
      unregisterAll();
    };
  }, []);

  // Removed unused applyApiKey()

  function promptForApiKey() {
    invoke<string | null>("get_gemini_api_key")
      .then((existing) => {
        const entered = window.prompt(
          existing ? "Update Gemini API Key (leave blank to clear)" : "Set Gemini API Key",
          existing ?? ""
        );
        if (entered === null) return; // cancelled
        invoke("set_gemini_api_key", { key: entered })
          .then(() => {
            console.log(existing ? "API key updated" : "API key set");
          })
          .catch((e) => console.error("Failed to set API key", e));
      })
      .catch((e) => console.error("Failed to read API key", e));
  }

  function promptForHfToken() {
    invoke<string | null>("get_hf_token")
      .then((existing) => {
        const entered = window.prompt(
          existing ? "Update Hugging Face Token (leave blank to clear)" : "Set Hugging Face Token for BEAST MODE",
          existing ?? ""
        );
        if (entered === null) return; // cancelled
        invoke("set_hf_token", { token: entered })
          .then(() => {
            console.log(existing ? "Hugging Face token updated" : "Hugging Face token set");
          })
          .catch((e) => console.error("Failed to set Hugging Face token", e));
      })
      .catch((e) => console.error("Failed to read Hugging Face token", e));
  }

  async function applyModel(newModel: string) {
    try {
      const m = await invoke<string>("set_model", { model: newModel });
      setModel(m);
    } catch (e) {
      console.error("Failed to set model", e);
    }
  }

  // Removed unused mouse handlers and empty effect

  return (
    <div 
      className="p-6 shadow-2xl text-white flex flex-col gap-4 relative app-shell"
      style={{ backgroundColor: `rgba(0,0,0,${opacity})`, ['--ui-opacity' as any]: opacity }}
    >
      <Textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        placeholder="Prompt here"
        className="bg-white/5 border border-white/10 placeholder:text-white/30 text-sm min-h-[100px] no-scrollbar"
        style={{ opacity }}
      />

      <div className="flex gap-2 items-center flex-wrap" style={{ opacity }}>
        <Button onClick={handleSendAllImages} disabled={loading || queueLength === 0}>
          {loading ? "Processing..." : `Ask (${queueLength} images)`}
        </Button>
        <Button variant="outline" onClick={handleAddImageToQueue}>
          Add Image (H)
        </Button>
        <Button variant="outline" onClick={handleClearQueue}>
          Clear (R)
        </Button>
        <Button variant="outline" onClick={promptForApiKey} title="Set Gemini API Key">
          Set Gemini Key
        </Button>
        <Button variant="outline" onClick={promptForHfToken} title="Set Hugging Face Token for BEAST MODE">
          Set HuggingFace Key
        </Button>
        <div className="basis-full h-0"></div>
        <select
          value={model}
          onChange={(e) => applyModel(e.target.value)}
          className="bg-white/5 border border-white/10 text-xs px-2 py-1 rounded text-white"
          style={{ opacity }}
        >
          <option value="gemini-2.5-pro">gemini-2.5-pro</option>
          <option value="gemini-2.5-flash">gemini-2.5-flash</option>
          <option value="gemini-2.0-flash">gemini-2.0-flash</option>
          
        </select>
        <select
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
          className="bg-white/5 border border-white/10 text-xs px-2 py-1 rounded text-white"
          style={{ opacity }}
        >
          <option value="C++">C++</option>
          <option value="C">C</option>
          <option value="Python">Python</option>
          <option value="Java">Java</option>
          <option value="JavaScript">JavaScript</option>
          <option value="TypeScript">TypeScript</option>
          <option value="Go">Go</option>
          <option value="Rust">Rust</option>
          <option value="C#">C#</option>
          <option value="Kotlin">Kotlin</option>
        </select>
        <select
          value={outputFormat}
          onChange={(e) => { const v = e.target.value; setOutputFormat(v); setPrompt(promptTemplates[v]); }}
          className="bg-white/5 border border-white/10 text-xs px-2 py-1 rounded text-white"
          style={{ opacity }}
        >
          <option value="single-correct-mcq">single-correct-mcq</option>
          <option value="multiple-correct-mcq">multiple-correct-mcq</option>
          <option value="code-without-comments">code-without-comments</option>
          <option value="code-with-explanation">code-with-explanation</option>
          <option value="beast-mode">BEAST MODE!!</option>
        </select>
      </div>

      <div
        className="markdown-body bg-white/5  p-4 text-sm overflow-y-auto flex-1 no-scrollbar"
        style={{ opacity }}
      >
        {output ? (
          <ReactMarkdown
            components={{
              code: (p: any) => {
                const { inline, className, children } = p ?? {};
                const codeText = String(children ?? "");
                if (inline) {
                  return <code className={className}>{children}</code>;
                }
                async function copyCode() {
                  try {
                    await navigator.clipboard.writeText(codeText);
                  } catch (e) {
                    console.error("Copy failed", e);
                  }
                }
                return (
                  <div className="relative">
                    <button
                      onClick={copyCode}
                      className="absolute top-2 right-2 bg-white/10 hover:bg-white/20 border border-white/20 text-white text-xs px-2 py-1 rounded"
                    >
                      Copy
                    </button>
                    <pre className={className}>
                      <code>{codeText}</code>
                    </pre>
                  </div>
                );
              },
            }}
          >
            {output}
          </ReactMarkdown>
        ) : (
          <span className="text-white/40">Response will appear here…</span>
        )}
      </div>

      <p className="text-xs text-white/50" style={{ opacity }}>
        ⌨️ Shortcuts: Ctrl+Shift+H (Add image), Ctrl+Shift+Enter (Send queue), Ctrl+Shift+R (Clear), Ctrl+Shift+] (Opacity up), Ctrl+Shift+[ (Opacity down), Ctrl+Shift+Arrow Keys (Move window), Ctrl+Shift+B (Toggle visibility), Ctrl+Shift+Q (Quit)
      </p>
      <p className="text-[10px] text-white/40" style={{ opacity }}>
        After setting the API key, wait 1–2 minutes for the system to update environment variables and optimize.
      </p>
      
      {/* No custom resize handle; allow native window/content sizing */}
    </div>
  );
}