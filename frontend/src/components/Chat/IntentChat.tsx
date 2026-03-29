import React, { useRef, useEffect, useState } from 'react';
import { Bot, Send } from 'lucide-react';
import { useBeaverStore } from '../../store';
import { MessageBubble } from './MessageBubble';
import { SpecCard } from './SpecCard';

let taskIdCounter = 0;

export const IntentChat: React.FC = () => {
  const messages = useBeaverStore((s) => s.messages);
  const generatedSpec = useBeaverStore((s) => s.generatedSpec);
  const addMessage = useBeaverStore((s) => s.addMessage);
  const setGeneratedSpec = useBeaverStore((s) => s.setGeneratedSpec);
  const addTask = useBeaverStore((s) => s.addTask);
  const setCurrentView = useBeaverStore((s) => s.setCurrentView);
  const resetChat = useBeaverStore((s) => s.resetChat);
  const sendOp = useBeaverStore((s) => s.sendOp);
  const activeTaskId = useBeaverStore((s) => s.activeTaskId);

  const [input, setInput] = useState('');
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, generatedSpec]);

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;

    const content = input;
    addMessage({ role: 'user', content });
    setInput('');

    // Send UserMessage Op via WebSocket if we have an active task
    if (activeTaskId && sendOp) {
      sendOp({ type: 'UserMessage', payload: { task_id: activeTaskId, content } });
    }

    // If enough context gathered and no spec yet, generate one from the conversation
    // (The real spec would come from AgentOutput events; this is a local UX convenience)
    if (messages.length >= 2 && !generatedSpec) {
      setGeneratedSpec({
        title: 'New Pipeline Task',
        description: content,
        workspace: 'New: auto-provisioned',
        techStack: ['Rust', 'TypeScript'],
      });
      addMessage({
        role: 'agent',
        content: 'Specification generated. Review the card below to deploy the pipeline.',
      });
    }
  };

  const handleDeploy = () => {
    if (!generatedSpec || !sendOp) return;

    // Create a temporary task ID for optimistic UI update
    taskIdCounter += 1;
    const taskId = `task_${Date.now()}_${taskIdCounter}`;

    // Optimistically add the task to the store (will be updated by PipelineCreated event)
    addTask({
      id: taskId,
      title: generatedSpec.title,
      spec: generatedSpec.description,
      workspaceId: 'ws-new',
      pipelineId: null,
      currentStage: 'created',
      status: 'initializing',
      logs: ['[system] Starting pipeline...'],
      priority: 'medium',
    });

    // Send StartPipeline Op to the backend -- the backend will respond with
    // PipelineCreated and StageTransition events that update the task
    sendOp({ type: 'StartPipeline', payload: { task_id: taskId, workspace_id: 'ws-new' } });

    resetChat();
    setCurrentView('dashboard');
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
            <h2 className="text-md font-bold text-white tracking-tight">Intent Clarifier</h2>
          </div>
          <div className="flex gap-1 items-center bg-indigo-500/10 px-3 py-1 rounded-full">
            <div className="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-pulse" />
            <span className="text-[10px] font-bold text-indigo-400 tracking-widest uppercase">Pipeline Ready</span>
          </div>
        </div>

        {/* Messages */}
        <div ref={scrollRef} className="flex-1 overflow-y-auto space-y-6 pr-4 mb-4 no-scrollbar">
          {messages.map((msg, i) => (
            <MessageBubble key={i} message={msg} />
          ))}
          {generatedSpec && <SpecCard spec={generatedSpec} onDeploy={handleDeploy} />}
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
};
