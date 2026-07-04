import { createRoot } from 'react-dom/client';

import './arco-compat';
import '@arco-design/web-react/dist/css/arco.css';
import { App } from './app/App';
import './index.css';

// pi0 is a dark-themed workbench — activate Arco's dark palette globally.
document.body.setAttribute('arco-theme', 'dark');

const container = document.getElementById('root');
if (container) {
    createRoot(container).render(<App />);
}
