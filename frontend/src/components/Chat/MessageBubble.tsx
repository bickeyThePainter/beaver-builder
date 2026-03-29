import React from 'react';
import type { ChatMessage } from '../../types';

interface Props {
  message: ChatMessage;
}

export const MessageBubble: React.FC<Props> = ({ message }) => {
  const isUser = message.role === 'user';

  return (
    <div className={`flex ${isUser ? 'justify-end' : 'justify-start'}`}>
      <div
        className={`max-w-[80%] p-4 rounded-2xl text-sm ${
          isUser
            ? 'bg-indigo-600 text-white rounded-tr-none'
            : 'bg-slate-900 border border-slate-800 text-slate-300 rounded-tl-none shadow-sm'
        }`}
      >
        {message.content}
      </div>
    </div>
  );
};
