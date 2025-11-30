import { invoke } from '@tauri-apps/api/core';

// Re-export invoke for convenience
export { invoke };

// Type-safe wrapper for checking sqlite-vec
export async function checkSqliteVec(): Promise<string> {
  return invoke<string>('check_sqlite_vec');
}

// Semantic search
export async function searchAtomsSemantic(
  query: string,
  limit: number = 20,
  threshold: number = 0.3
): Promise<any[]> {
  return invoke('search_atoms_semantic', { query, limit, threshold });
}

// Find similar atoms
export async function findSimilarAtoms(
  atomId: string,
  limit: number = 5,
  threshold: number = 0.7
): Promise<any[]> {
  return invoke('find_similar_atoms', { atomId, limit, threshold });
}

// Retry embedding
export async function retryEmbedding(atomId: string): Promise<void> {
  return invoke('retry_embedding', { atomId });
}

// Process pending embeddings
export async function processPendingEmbeddings(): Promise<number> {
  return invoke('process_pending_embeddings');
}

// Get embedding status
export async function getEmbeddingStatus(atomId: string): Promise<string> {
  return invoke('get_embedding_status', { atomId });
}

// Wiki commands
export async function getWikiArticle(tagId: string): Promise<any | null> {
  return invoke('get_wiki_article', { tagId });
}

export async function getWikiArticleStatus(tagId: string): Promise<any> {
  return invoke('get_wiki_article_status', { tagId });
}

export async function generateWikiArticle(tagId: string, tagName: string): Promise<any> {
  return invoke('generate_wiki_article', { tagId, tagName });
}

export async function updateWikiArticle(tagId: string, tagName: string): Promise<any> {
  return invoke('update_wiki_article', { tagId, tagName });
}

export async function deleteWikiArticle(tagId: string): Promise<void> {
  return invoke('delete_wiki_article', { tagId });
}

// Canvas position commands
export interface AtomPosition {
  atom_id: string;
  x: number;
  y: number;
}

export interface AtomWithEmbedding {
  id: string;
  content: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: string;
  tags: Array<{
    id: string;
    name: string;
    parent_id: string | null;
    created_at: string;
  }>;
  embedding: number[] | null;
}

export async function getAtomPositions(): Promise<AtomPosition[]> {
  return invoke('get_atom_positions');
}

export async function saveAtomPositions(positions: AtomPosition[]): Promise<void> {
  return invoke('save_atom_positions', { positions });
}

export async function getAtomsWithEmbeddings(): Promise<AtomWithEmbedding[]> {
  return invoke('get_atoms_with_embeddings');
}

// Semantic graph types and commands
export interface SemanticEdge {
  id: string;
  source_atom_id: string;
  target_atom_id: string;
  similarity_score: number;
  source_chunk_index: number | null;
  target_chunk_index: number | null;
  created_at: string;
}

export interface NeighborhoodAtom {
  id: string;
  content: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: string;
  tags: Array<{
    id: string;
    name: string;
    parent_id: string | null;
    created_at: string;
  }>;
  depth: number; // 0 = center, 1 = direct connection, 2 = friend-of-friend
}

export interface NeighborhoodEdge {
  source_id: string;
  target_id: string;
  edge_type: 'tag' | 'semantic' | 'both';
  strength: number; // 0-1
  shared_tag_count: number;
  similarity_score: number | null;
}

export interface NeighborhoodGraph {
  center_atom_id: string;
  atoms: NeighborhoodAtom[];
  edges: NeighborhoodEdge[];
}

export async function getSemanticEdges(minSimilarity: number = 0.5): Promise<SemanticEdge[]> {
  return invoke('get_semantic_edges', { minSimilarity });
}

export async function getAtomNeighborhood(
  atomId: string,
  depth: number = 1,
  minSimilarity: number = 0.5
): Promise<NeighborhoodGraph> {
  return invoke('get_atom_neighborhood', { atomId, depth, minSimilarity });
}

export async function rebuildSemanticEdges(): Promise<number> {
  return invoke('rebuild_semantic_edges');
}

// Clustering types and commands
export interface AtomCluster {
  cluster_id: number;
  atom_ids: string[];
  dominant_tags: string[];
}

export async function computeClusters(
  minSimilarity: number = 0.5,
  minClusterSize: number = 2
): Promise<AtomCluster[]> {
  return invoke('compute_clusters', { minSimilarity, minClusterSize });
}

export async function getClusters(): Promise<AtomCluster[]> {
  return invoke('get_clusters');
}

export async function getConnectionCounts(
  minSimilarity: number = 0.5
): Promise<Record<string, number>> {
  return invoke('get_connection_counts', { minSimilarity });
}

// Model discovery types and commands
export interface AvailableModel {
  id: string;
  name: string;
}

export async function getAvailableLlmModels(): Promise<AvailableModel[]> {
  return invoke('get_available_llm_models');
}

