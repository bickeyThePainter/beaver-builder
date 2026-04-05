import { Bot, Send } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useStore } from '../../store';
import { MessageBubble } from './MessageBubble';
import { SpecCard } from './SpecCard';

export function PlannerChat() {
  const [input, setInput] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  const messages = useStore((s) => s.messages);
  const generatedSpec = useStore((s) => s.generatedSpec);
  const sendOp = useStore((s) => s.sendOp);
  const activeTaskId = useStore((s) => s.activeTaskId);
  const setActiveTaskId = useStore((s) => s.setActiveTaskId);
  const addMessage = useStore((s) => s.addMessage);
  const setView = useStore((s) => s.setView);
  const setSpec = useStore((s) => s.setSpec);
  const resetChat = useStore((s) => s.resetChat);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, generatedSpec]);

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || !sendOp) return;

    // Create task ID if not yet created
    let taskId = activeTaskId;
    if (!taskId) {
      taskId = `task-${Date.now()}`;
      setActiveTaskId(taskId);
    }

    // Add user message to local chat
    addMessage({ role: 'user', content: input });

    // Send through WebSocket
    sendOp({
      type: 'UserMessage',
      payload: { task_id: taskId, content: input },
    });

    setInput('');
  };

  const handleDeploy = () => {
    if (!sendOp || !activeTaskId) return;

    // Send StartPipeline through WebSocket
    sendOp({
      type: 'StartPipeline',
      payload: {
        task_id: activeTaskId,
        workspace_id: 'ws-default',
      },
    });

    // Reset chat and go to dashboard
    resetChat();
    setView('dashboard');
  };

  return (
    <div className="max-w-4xl mx-auto flex flex-col h-[70vh]">
      <div className="flex-1 bg-slate-950/50 border border-slate-800 rounded-3xl p-6 flex flex-col overflow-hidden shadow-inner">
        {/* Header */}
        <div className="flex items-center justify-between mb-6 border-b border-slate-800 pb-4">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-full bg-indigo-600 flex items-center justify-center">
              <Bot size={24} />
            </div>
            <h2 className="text-md font-bold text-white tracking-tight">
              Planner Agent
            </h2>
          </div>
          <div className="flex gap-1 items-center bg-indigo-500/10 px-3 py-1 rounded-full">
            <div className="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-pulse" />
            <span className="text-[10px] font-bold text-indigo-400 tracking-widest uppercase">
              Pipeline v2 Ready
            </span>
          </div>
        </div>

        {/* Messages */}
        <div
          ref={scrollRef}
          className="flex-1 overflow-y-auto space-y-6 pr-4 mb-4"
        >
          {messages.map((msg, i) => (
            <MessageBubble key={i} message={msg} />
          ))}
          {generatedSpec && (
            <SpecCard spec={generatedSpec} onDeploy={handleDeploy} />
          )}
        </div>

        {/* Input */}
        <form onSubmit={handleSend} className="relative">
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Describe your coding objective..."
            className="w-full bg-slate-900 border border-slate-800 rounded-2xl pl-6 pr-14 py-4 text-sm focus:border-indigo-500 outline-none transition-all placeholder:text-slate-600 shadow-2xl"
          />
          <button
            type="submit"
            className="absolute right-3 top-1/2 -translate-y-1/2 p-2 bg-indigo-600 text-white rounded-xl shadow-md active:scale-95 transition-transform"
          >
            <Send size={18} />
          </button>
        </form>
      </div>
    </div>
  );
}
