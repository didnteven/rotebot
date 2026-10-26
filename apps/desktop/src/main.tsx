import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { createBackend } from './backend';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('App root is missing');

createRoot(root).render(
  <StrictMode>
    <App backend={createBackend()} />
  </StrictMode>,
);
