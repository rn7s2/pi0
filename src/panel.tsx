import { createRoot } from 'react-dom/client';

import { FloatPanel } from './app/FloatPanel';
import './panel.css';

const container = document.getElementById('root');
if (container) {
  createRoot(container).render(<FloatPanel />);
}
