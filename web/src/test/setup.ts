import "@testing-library/jest-dom";

// jsdom does not implement scroll APIs — stub them so ChatView's
// useAutoScroll and scrollIntoView calls don't throw in tests.
window.HTMLElement.prototype.scrollTo = () => {};
window.HTMLElement.prototype.scrollIntoView = () => {};
