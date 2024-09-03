import { SocketWrapper } from "../index";

describe("default", () => {
  let socket: SocketWrapper;

  beforeEach(() => {
    socket = new SocketWrapper();
  });

  it("basic", () => {
    expect(socket.isConnect()).toBe(false);
  });
});
