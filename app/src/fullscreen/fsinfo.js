// The core's lists for the fullscreen mode, from settings_info: the
// details it can show (fsdetails.js) and the bookshelf's choices
// (fsshelf.js). Called wherever settings_info is read.

import { knowDetails } from "./fsdetails.js";
import { knowShelf } from "./fsshelf.js";

export function knowFsInfo(info) {
  knowDetails(info?.fs_details);
  knowShelf(info?.fs_bookshelf);
}
