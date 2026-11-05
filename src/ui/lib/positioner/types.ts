export type Side = "top" | "right" | "bottom" | "left";
export type Alignment = "start" | "end";
export type AlignedPlacement = `${Side}-${Alignment}`;
export type Placement = Side | AlignedPlacement;
export type Strategy = "absolute" | "fixed";
export type Axis = "x" | "y";

export interface Coords {
  x: number;
  y: number;
}

export interface Rect extends Coords {
  width: number;
  height: number;
}

export interface ElementRects {
  reference: Rect;
  floating: Rect;
}

export interface Elements {
  reference: ReferenceElement;
  floating: HTMLElement;
}

export interface VirtualElement {
  getBoundingClientRect(): Rect;

  contextElement?: Element;
}

export type ReferenceElement = Element | VirtualElement;

export interface MiddlewareData {
  [key: string]: any;
}

export interface MiddlewareState extends Coords {
  initialPlacement: Placement;
  placement: Placement;
  strategy: Strategy;
  rects: ElementRects;
  elements: Elements;
  middlewareData: MiddlewareData;
}

export interface MiddlewareReturn extends Partial<Coords> {
  data?: any;

  reset?: boolean | { placement?: Placement };
}

export interface Middleware {
  name: string;
  fn(state: MiddlewareState): MiddlewareReturn;
}

export interface ComputePositionConfig {
  placement?: Placement;
  strategy?: Strategy;
  middleware?: Middleware[];
}

export interface ComputePositionReturn extends Coords {
  placement: Placement;
  strategy: Strategy;
  middlewareData: MiddlewareData;
}

export interface Boundary extends Rect {}
