package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.ImageInfo

/**
 * Images by folder, for the Reader's list. QA runs keep their screenshots in nested
 * folders, so a tree stays short where a flat list would not.
 */
data class ImageFolder(
    /** The folder's path in the repo; "" for the top. */
    val path: String,
    val name: String,
    /** Subfolders, then images, each in name order. */
    val folders: List<ImageFolder> = emptyList(),
    val images: List<ImageInfo> = emptyList(),
) {
    /** Images in this folder and all below it. */
    val count: Int get() = images.size + folders.sumOf { it.count }

    companion object {
        /**
         * The tree of [images]. A chain of folders with one subfolder and no images is
         * joined into one node, e.g. `build/screenshots`.
         */
        fun tree(images: List<ImageInfo>): ImageFolder = joined(build("", "", images))

        /** The images next to [path], in name order: what Previous and Next step through. */
        fun siblings(path: String, images: List<ImageInfo>): List<ImageInfo> {
            val folder = folder(path)
            return images.filter { folder(it.path) == folder }.sortedBy { it.path }
        }

        /** The folders from the top to [path]'s folder, e.g. to open them. */
        fun ancestors(path: String): List<String> {
            val parts = path.split('/').filter { it.isNotEmpty() }.dropLast(1)
            return parts.indices.map { parts.subList(0, it + 1).joinToString("/") }
        }

        internal fun folder(path: String) = path.split('/').filter { it.isNotEmpty() }.dropLast(1).joinToString("/")

        private fun build(path: String, name: String, images: List<ImageInfo>): ImageFolder {
            val prefix = if (path.isEmpty()) "" else "$path/"
            val here = images.filter { folder(it.path) == path }.sortedBy { it.path }
            val below = images.filter { it.path.startsWith(prefix) && folder(it.path) != path }
            val children = below.groupBy { it.path.removePrefix(prefix).substringBefore('/') }
                .toSortedMap()
                .map { (child, list) -> build(prefix + child, child, list) }
            return ImageFolder(path, name, children, here)
        }

        private fun joined(folder: ImageFolder): ImageFolder = folder.copy(
            folders = folder.folders.map { f ->
                var node = f
                while (node.images.isEmpty() && node.folders.size == 1) {
                    val only = node.folders.single()
                    node = ImageFolder(only.path, "${node.name}/${only.name}", only.folders, only.images)
                }
                joined(node)
            },
        )
    }
}
