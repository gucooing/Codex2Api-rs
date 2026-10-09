"use client";
import { useActions, useErrorToast } from "@/lib/actions";
import { request, type Session } from "@/lib/api";
import { useResource } from "@/lib/resource";
import { useUserSession } from "@/lib/session";
import { useId, useState } from "react";
import { toast } from "sonner";

export function useProfilePage() {
  const { session, signOut } = useUserSession();
  const profile = useResource<Session>("/session", 0);
  const user = profile.data?.user ?? session.user;
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const id = useId();
  const actions = useActions();
  useErrorToast(profile.error);
  const handleSubmit = (event: React.SubmitEvent<HTMLFormElement>) =>
    actions.submit(
      event,
      "password",
      async () => {
        await request("/password", {
          method: "POST",
          body: { current_password: currentPassword, new_password: newPassword },
        });
        signOut();
        toast.success("密码已更新，请重新登录");
      },
      "",
    );
  return {
    profile,
    user,
    currentPassword,
    setCurrentPassword,
    newPassword,
    setNewPassword,
    id,
    actions,
    handleSubmit,
  } as const;
}
