import {
  createLaunchProfile,
  deleteLaunchProfile,
  duplicateLaunchProfile,
  getLaunchProfiles,
  updateLaunchProfile,
  type LaunchOverrides,
  type LaunchProfiles,
} from "../services/launch-profiles";
import { createResource } from "./resource";

export const launchProfiles = createResource<LaunchProfiles>({ profiles: [], lastUsed: {} }, getLaunchProfiles);

async function apply(task: Promise<LaunchProfiles>): Promise<void> {
  launchProfiles.set(await task);
}

export function addLaunchProfile(gameId: string, name: string, overrides: LaunchOverrides): Promise<void> {
  return apply(createLaunchProfile(gameId, name, overrides));
}

export function saveLaunchProfile(id: string, name: string, overrides: LaunchOverrides): Promise<void> {
  return apply(updateLaunchProfile(id, name, overrides));
}

export function copyLaunchProfile(id: string, name: string): Promise<void> {
  return apply(duplicateLaunchProfile(id, name));
}

export function removeLaunchProfile(id: string): Promise<void> {
  return apply(deleteLaunchProfile(id));
}
