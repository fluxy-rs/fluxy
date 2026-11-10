import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { PageTitle } from "./PageTitle";

describe("PageTitle", () => {
  it("renders the page heading with the shared display size", () => {
    render(() => <PageTitle title="设备" subtitle="同一局域网内的受信设备" />);

    const heading = screen.getByRole("heading", { level: 1, name: "设备" });
    expect(heading).toHaveClass("text-display");
    expect(screen.getByText("同一局域网内的受信设备")).toBeInTheDocument();
  });

  it("places the aside beside the heading", () => {
    render(() => <PageTitle title="设置" aside={<span>v0.2.0</span>} />);

    expect(screen.getByText("v0.2.0")).toBeInTheDocument();
  });

  it("renders no subtitle when there is none", () => {
    const { container } = render(() => <PageTitle title="帮助" />);

    expect(container.querySelectorAll("p")).toHaveLength(0);
  });
});
