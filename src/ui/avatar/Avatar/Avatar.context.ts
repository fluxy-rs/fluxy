import { createContext, useContext, type Accessor, type Setter } from "solid-js";

interface AvatarContextValue {
  imageLoadFailed: Accessor<boolean>;
  setImageLoadFailed: Setter<boolean>;
  /**
   */
  imagePresent: Accessor<boolean>;
  setImagePresent: Setter<boolean>;
}

export const AvatarContext = createContext<AvatarContextValue>();

export const useAvatarContext = () => {
  const ctx = useContext(AvatarContext);
  if (!ctx) {
    throw new Error("useAvatarContext must be used inside <Avatar>");
  }

  return ctx;
};
