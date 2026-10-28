import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach, beforeEach, vi } from 'vitest';

afterEach(cleanup);

beforeEach(() => { vi.spyOn(window, 'scrollTo').mockImplementation(() => {}); });
