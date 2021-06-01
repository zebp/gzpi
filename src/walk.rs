use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::Result;
use async_walkdir::WalkDir;
use futures::StreamExt;
use id_tree::{InsertBehavior, Node, NodeId, Tree, TreeBuilder};

#[derive(Debug, Clone)]
pub struct Item {
    path: PathBuf,
    is_file: bool,
}

// TODO: Respect git ignore
async fn walk_dir(path: &Path, _use_git_ignore: bool) -> Result<Tree<Item>> {
    let mut walker = WalkDir::new(path);
    let mut items = Vec::new();

    while let Some(entry) = walker.next().await {
        let entry = entry?;
        let is_file = entry.metadata().await?.is_file();
        items.push(Item {
            path: entry.path(),
            is_file,
        });
    }

    // Sorts by the number of parent directories so we always have a node for that parent directory
    // in the tree before we try to create the node.
    items.sort_unstable_by_key(|item| item.path.iter().count());

    // A map of path names to node ids to reconstruct the tree from the flat list of items.
    let mut node_id_map: HashMap<String, NodeId> = HashMap::new();
    // Converts a path to the a stringified version without a trialing slash.
    let path_key = |path: &Path| {
        path.iter()
            .filter_map(OsStr::to_str)
            .map(String::from)
            .collect::<Vec<_>>()
            .join("/")
    };

    let mut tree = Tree::new();
    let root_id = tree.insert(
        Node::new(Item {
            path: path.into(),
            is_file: false,
        }),
        InsertBehavior::AsRoot,
    )?;
    node_id_map.insert(path_key(path), root_id);

    for item in items {
        let parent = item.path.parent().expect("item doesn't have parent path");
        let parent_id = node_id_map
            .get(&path_key(parent))
            .expect("parent not in node id map");

        let node_id_key = path_key(&item.path);
        let node_id = tree.insert(Node::new(item), InsertBehavior::UnderNode(&parent_id))?;

        // TODO: Figure out a way to do this more elegantly
        tree.sort_children_by_key(parent_id, |node| path_key(&node.data().path))?;

        node_id_map.insert(node_id_key, node_id);
    }

    return Ok(tree);
}

// TODO: Respect git ignore
#[allow(dead_code)]
pub async fn create_file_tree(path: &Path, use_git_ignore: bool) -> Result<Tree<Item>> {
    Ok(if path.is_file() {
        TreeBuilder::new()
            .with_root(Node::new(Item {
                path: path.into(),
                is_file: true,
            }))
            .build()
    } else {
        walk_dir(path, use_git_ignore).await?
    })
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn tree_from_dirs() {
        let tree = create_file_tree(Path::new("testdata"), false)
            .await
            .unwrap();
        let mut paths = Vec::new();
        let root_id = tree.root_node_id().unwrap();

        for node in tree.traverse_pre_order(root_id).unwrap() {
            paths.push(node.data().path.as_path());
        }

        assert_eq!(
            &[
                Path::new("testdata/"),
                Path::new("testdata/a"),
                Path::new("testdata/a/b"),
                Path::new("testdata/a/b/c"),
                Path::new("testdata/a/b/c/.gitkeep"),
                Path::new("testdata/a/b/d"),
                Path::new("testdata/a/b/d/.gitkeep"),
                Path::new("testdata/a/e"),
                Path::new("testdata/a/e/.gitkeep"),
                Path::new("testdata/a/f")
            ],
            paths.as_slice()
        )
    }

    #[tokio::test]
    async fn tree_from_file() {
        let tree = create_file_tree(Path::new("testdata/a/f"), false)
            .await
            .unwrap();
        let mut paths = Vec::new();
        let root_id = tree.root_node_id().unwrap();

        for node in tree.traverse_pre_order(root_id).unwrap() {
            paths.push(node.data().path.as_path());
        }

        assert_eq!(paths.len(), 1);
        assert_eq!(Path::new("testdata/a/f"), paths[0]);
    }
}
