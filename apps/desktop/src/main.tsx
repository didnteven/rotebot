import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { createBackend } from './backend';
import './styles.css';

// The saved appearance applies before the first paint; Settings changes it.
try { document.documentElement.dataset.theme = localStorage.getItem('macroscope.theme') || 'system'; } catch { /* System appearance. */ }

const root = document.getElementById('root');
if (!root) throw new Error('App root is missing');

createRoot(root).render(
  <StrictMode>
    <App backend={createBackend()} />
  </StrictMode>,
);
