import type { CatalogDocument } from "./types";

export interface CatalogTreeMember {
  kind: string;
  name: string;
  detail?: string;
}

export interface CatalogTreeNode {
  key: string;
  name: string;
  kind?: string;
  detail?: string;
  members?: CatalogTreeMember[];
  children: CatalogTreeNode[];
}

export function buildCatalogTree(catalog: CatalogDocument): CatalogTreeNode[] {
  const roots: CatalogTreeNode[] = [];

  for (const object of catalog.objects ?? []) {
    let siblings = roots;
    const path: string[] = [];

    object.name.forEach((part, index) => {
      path.push(part);
      let node = siblings.find((candidate) => candidate.name === part);
      if (!node) {
        node = {
          key: path.join("."),
          name: part,
          children: [],
        };
        siblings.push(node);
      }

      if (index === object.name.length - 1) {
        node.kind = object.kind;
        node.detail = object.detail;
        node.members = object.members;
      }

      siblings = node.children;
    });
  }

  return roots;
}
